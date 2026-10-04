# tasu — 架构

本文由 `PRODUCT.md` 倒推而来。产品是唯一的约束来源；架构里的每个模块都必须能回答"它是为了满足产品的哪一条"。

## 设计原则

1. **领域纯，时间注入**：`今天/本周/以后` 的自动降级是产品的心脏，必须能用固定时间戳测试，不碰终端、不碰系统时钟。
2. **UI 是状态的函数**：同一份 `Model` 渲染成窄屏三段或宽屏三列，渲染只读。
3. **I/O 出主循环**：git 是网络操作、文件可能被外部改；二者都不得阻塞按键。
4. **两个前端一个核心**：TUI 与 `tasu add` 共享 `domain` + `store` + `sync`。
5. **依赖最小**：只新增 `clap`、`chrono`。git 走系统命令，文件侦测走 mtime 轮询。

## 分层

```
main.rs ──▶ cli.rs (clap: 无子命令→TUI, add→命令)
                │
       ┌────────┴────────┐
       ▼                 ▼
   app/ + ui/        command/
   Event→Action→      (一次性: 写 + 尽力 push)
   update→Effect
       └────────┬────────┘
                ▼
            domain/            ← 纯逻辑，零 I/O，注入 now
       Board · Task · settle()
                │
       ┌────────┴────────┐
       ▼                 ▼
    store/            sync/
   (JSON 原子写)     (git shell + 后台线程)

主循环:
terminal.draw(render(&model))
  ├─ event::poll(tick) ? read() → Action : Tick
  ├─ update(&mut model, action) → Vec<Effect>
  └─ effect::run(effects, &mut model, &runtime)   // 落地 I/O，结果回流成 Action
```

```
src/
  main.rs            入口：解析 CLI 并分发
  cli.rs             clap 定义
  command/mod.rs     `add` 等一次性命令
  domain/
    task.rs          Task / TaskState / Bucket
    board.rs         Board：集合 + 操作 + 按桶查询
    settle.rs        纯降级 settle(&mut Board, now)
  store/mod.rs       加载 / 原子保存 / mtime / schema 版本
  sync/mod.rs        Git：pull、commit+push，后台线程 + 结果 channel
  app/
    model.rs         Model = Board + UiState
    action.rs        Action 枚举
    update.rs        update(&mut Model, Action) -> Vec<Effect>
    event.rs         crossterm Event -> Action
    effect.rs        Effect 落地执行
  ui/
    mod.rs           按宽度选布局 + 组装 + toast/modal
    theme.rs         冷色调
    sections.rs      窄屏纵向三段
    kanban.rs        宽屏三列
    modal.rs         已完成 / 帮助
    components.rs    条目、头部（ISO 周）、toast
```

## 数据契约（磁盘）

单文件 `todos.json`，位于数据目录（默认平台数据目录，`TASU_DATA` 可覆盖）。**Schema 带版本号**，为将来迁移留门。

```json
{
  "version": 1,
  "tasks": [
    {
      "title": "Nand2Tetris 第六章",
      "state": "open",              // open | done | archived
      "bucket": "today",            // today | week | later
      "bucket_since": "2026-10-04T09:12:00.123+08:00",
      "created_at":   "2026-10-04T09:12:00.123+08:00",
      "completed_at": null,
      "archived_at":  null
    }
  ]
}
```

关键设计决定：

- **无 ID**。单写者 + 无并发合并，操作全程用位置定位；桶内顺序由 `created_at`（含纳秒）派生，因此**恢复任务 = 改回原 `bucket` 并重置 `bucket_since`，排序自然回到原位**，不需要额外记位置。
- **`bucket` 在 `done`/`archived` 时保留**：撤销/找回时才有"原桶"可用。
- 时间一律 `chrono::DateTime<Local>`。

## 领域与状态机

```rust
// domain/task.rs
enum TaskState { Open, Done, Archived }
enum Bucket    { Today, Week, Later }

struct Task {
    title: String,
    state: TaskState,
    bucket: Bucket,
    bucket_since: DateTime<Local>,  // 进入当前桶的时刻
    created_at: DateTime<Local>,
    completed_at: Option<DateTime<Local>>,
    archived_at: Option<DateTime<Local>>,
}
```

```rust
// domain/settle.rs — 纯函数，时间注入
fn settle(board: &mut Board, now: DateTime<Local>) {
    for t in board.open_mut() {
        match t.bucket {
            Today if t.bucket_since.date_naive() < now.date_naive()
                => { t.bucket = Week; t.bucket_since = now; }
            Week if iso_week(t.bucket_since) < iso_week(now)
                => { t.bucket = Later; t.bucket_since = now; }
            _ => {}
        }
    }
}
```

- `iso_week` 比较 `(ISO year, week)`，处理跨年周（第 1 周可能属于上一年）。
- **只有 `Open` 会老化**；`Done` / `Archived` 冻结。
- `settle` 在**加载后**和**每次 Tick** 调用；产生变化才触发保存。

`Board` 操作（全部接收 `now`）：`add` / `complete` / `archive` / `restore`（done→open，重置 `bucket_since=now`）/ `unarchive` / `move_bucket(+1|-1)` / `pin_today` / `rename`；查询：`open_in(bucket)` 按 `created_at` 倒序、`done()`。

## Effect 清单

`update` 不碰 I/O，只返回下列之一：

| Effect | 含义 |
| --- | --- |
| `Save` | 原子写 `todos.json` |
| `Sync` | 标记 dirty；由 Tick 的 debounce 决定何时 commit+push |
| `PullThenLoad` | 启动时先 `git pull`，再加载并 `settle` |
| `Quit` | 退出（退出前保底 push） |

## 主循环与并发

- 事件循环：`draw → poll(tick) → read/ Tick → update → run effects`。
- **Tick**（约 200ms）负责：文件 mtime 侦测（外部改动 → `Action::FileChanged` 重载并 `settle`）、git debounce 触发、toast 过期。
- **sync 后台线程**：`Effect::Sync` 只更新 `dirty_since`；Tick 发现"dirty 且静默 > 3s"才 spawn 线程执行 `git add/commit/push`，结果经 `mpsc` 变 `Action::SyncFinished`。乐观确认：push 后按 generation 决定是否清 dirty（push 期间的新改动不丢）。
- 不引入 tokio：只有一个慢操作 + 一个轮询，标准库线程足够。

## UI 契约

- `ui::draw(f, &Model)`：容器 → 头部 → 主体 → 底部 → 浮层（modal / toast）。**只读 `Model`**。
- 布局阈值 `const KANBAN_MIN_WIDTH: u16`：`width >= 阈值` 走 `kanban`（三列），否则 `sections`（纵向三段）。
- 头部：`TODAY` 计数 + `WEEK <iso> · 今年还剩 <n> 周`。
- 颜色只做信号：选中行、`today`、顺延计数（`>= 3` 警示色）、完成/归档态。
- Toast：捕获后一闪 `已存入 今天 · 1`，约 2s 自动消失，不打断光标。
- `later` 折叠：默认只显示最近 N 条，其余折叠为 `更早的 <n> 条`。

## CLI

- `tasu` → 启动 TUI。
- `tasu add <title>...` → 加载 + `settle` + `add` + 保存 + **尽力** commit/push（带超时；离线不报错、退出码仍为 0）。
- 两个入口共用 `store` / `sync` / `domain`。

## 设置

**默认零配置**：`cargo install tasu` 之后直接跑，纯本地、不联网。设置只用于进阶用法。

| 设置 | 默认 | 覆盖方式 | 说明 |
| --- | --- | --- | --- |
| 数据目录 | 平台数据目录 `/tasu` | 环境变量 `TASU_DATA`，或配置文件 | 存放 `todos.json`；同步时它同时是 git 工作副本 |
| 同步 remote | 无（纯本地） | 环境变量 `TASU_REMOTE`，或配置文件 | Git 远端 URL；**未设置则完全不调用 git** |

- 配置文件：平台配置目录 `/tasu/config.json`（`dirs::config_dir()`），字段可选：`data_dir`、`remote`。**配置与数据分离**，配置不进数据仓库，避免随同步漂移。
- 优先级：环境变量 > 配置文件 > 默认值。
- 设置了 `remote` 时：首次运行若数据目录不是 git 仓库，则 `git init` + `git remote add origin <remote>` + 首次 commit，之后走正常 pull/push。

## 同步

Git 单写者模型（详见 `PRODUCT.md`）：`git -C <repo> pull --rebase --autostash`、`git add -A && git commit -m "tasu: <n> changes" && git push`。**未设置 remote / 非 git 仓库时整体静默降级为纯本地**，不产生任何 git 调用。所有失败静默留待下次重试。

## 测试

E2E 优先（ratatui `TestBackend` + 临时目录 + 本地裸仓库）：

- 按键 → 屏幕内容 / `todos.json` / git 历史。
- 时间边界：日切降级、ISO 周切降级（含跨年第 1 周）、`settle` 幂等、完成冻结不老化。
- 跨桶移动、撤销回原桶原位置、归档找回、`later` 折叠。
- 同步：无 remote 静默、离线 push 不阻塞退出。

> 遵循：若要隔离测试某系统，先写"它可能怎么失败"，再写代码。领域隔离测试只用于时间边界这类硬骨头，其余一律 E2E。
