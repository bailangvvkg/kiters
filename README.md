# kiters

一个站点,整个预渲染。kite 的 Rust 之子。

```
kiters build --site . --out dist
kiters serve --site . --addr 0.0.0.0:1718
```

```
我的站: 5 项(1 草稿) → 12 页,落盘 12 个文件,用时 2ms
```

## 是什么

读一个内容树(`content/<kind>/<slug>/index.md`,和 kite 站点同一形状),
预渲染**整站**——每篇文章页、首页、404、sitemap、RSS、别名页、税目页——
落成任何静态服务器都能直接伺候的文件。草稿不出现、定时未到不出现,
和 kite 的私密语义一致。

`serve` 把整站握成一个快照:每个回答都是同一份完整字节(响应头
`x-kiters: snapshot`),改文件→整站重建→原子换血(ArcSwap),
query 字符串天然免疫(路径即地址)。压缩协商 br/zstd/gzip,超过 1 KiB 才值得运。

## 钩子:能 Hook 一切

每个阶段都有事件,链式执行,第一个错误终止构建并**报出钩子名与时刻**:

| 事件 | 时刻 | 可改什么 |
|---|---|---|
| `on_discover` | 文件被发现,读之前 | `skip` 整个文件 |
| `on_front_matter` | front matter 解析后 | 填 slug/日期/状态 |
| `on_markdown` | 正文渲染前 | 改写 markdown(链接转卡片之类) |
| `on_target` | 目标入计划 | 改地址、改落盘路径、`skip` 撤页 |
| `on_html` | 正文成 HTML、进布局前 | 最后一眼 body |
| `on_page` | 整页完成 | 页面字节的最后决定权 |
| `on_build_complete` | 全站渲染完,落盘前 | 只读:站点、计划、页面、报告 |

## 插件:WebAssembly,七钩子,燃料沙箱

钩子不用编进 kiters——写一个 WebAssembly 组件(`kiters:plugin` 协议,
`crates/kiters-wasm/wit/kiters.wit`),`--plugin x.wasm` 挂上:

```
cargo build -p stamp-wasm --target wasm32-wasip2
kiters serve --site . --plugin ../../../target/wasm32-wasip2/debug/stamp_wasm.wasm
```

- 七个时刻全导出;不关心的原样返回(协议模板见 `plugins/` 三个样例)。
- 引擎共享、组件一次编译、实例复用;每次调用前**燃料重置**——
  插件跑死循环=构建报错,不是挂死。
- WASI 最小实现:无文件、无时钟、无随机。插件是变换,不是进程。

```rust
struct Card;

impl Hook for Card {
    fn on_markdown(&self, e: MarkdownEvent<'_>) -> anyhow::Result<()> {
        e.content.body = turn_bare_links_into_cards(&e.content.body);
        Ok(())
    }
}

let (_, plan, pages, report) = build_site(
    &SiteBuilder::new(site_dir).out(dist_dir).hook(Card)
)?;
```

## 布局

```
crates/
  kiters-core     类型(Kind/Status/FrontMatter/Target/Plan)+ Hook API
  kiters-parse    树遍历、front matter 拆分、钩子埋点
  kiters-render   comrak(GFM)+ minijinja 内置布局 + 短代码折叠
  kiters-build    计划与预渲染编排,落盘
  kiters-serve    axum 快照服务 + watch + 压缩
  kiters-wasm     wasmtime 49 宿主桥 + WIT 协议
plugins/          样例插件(stamp/skipper/linkcard-lite)
apps/
  kiters-cli      kiters build / serve
tests/fixture/    中文示例站(3 发布 + 1 草稿 + 1 定时,别名/税目/长文)
```

## 路线

- [x] M1 预渲染内核
- [x] M2 serve:快照换血 + watch + 压缩协商 + 短代码 + 别名页 + 税目页 + 定时发布
- [x] M3 wasmtime + WIT 组件插件(燃料沙箱,三个样例,linkcard 对拍)
- [x] M4 Studio admin API(--admin:登录会话/cursor 分页/If-Match 409/媒体/预览/导入导出)
- [x] M5 主题层 + Go-template 迁移器 + syntect 上色 + /search.json

设计全案见 `docs/LLM_HANDOFF.md`(前史在 kite 仓库 `kiters` 分支)。

## 热状图

```
kiters build --site . --cpuprofile cpu.pb --flamegraph flame.svg --stats
```

- `--cpuprofile`:Go pprof 格式(glibc Linux 上 pprof-rs 采样,`go tool pprof`
  可读;musl 静态二进制上信号内回溯空转,已知限制)
- `--flamegraph`:独立 SVG 火焰图
- `--stats`:构建数字 + 进程峰值内存(Linux 读 /proc VmHWM)
- musl 静态二进制上用 perf 代替(符号全开):
  `perf record -F 399 -g --call-graph fp -o perf.data \
    ./kiters build ... && perf report --stdio --no-children`

实测(服务器,400 篇放大站):热路径 = comrak 的 markdown→HTML
(memcpy/utf8/分配器占前三,共 ~17%),其余全是噪声——优化时只看这里。

## 测试

```
cargo build -p stamp-wasm -p skipper-wasm -p linkcard-lite --target wasm32-wasip2
cargo test --workspace   # 40+ 项:整站/CLI/钩子/serve/admin/主题/上色/迁移器/插件/短代码
```
