# kiters

一个站点,整个预渲染。kite 的 Rust 之子。

```
kiters build --site . --out dist
```

```
我的站: 3 项(1 草稿) → 6 页,落盘 6 个文件,用时 2ms
```

## 是什么

读一个内容树(`content/<kind>/<slug>/index.md`,和 kite 站点同一形状),
预渲染**整站**——每篇文章页、首页、404、sitemap、RSS——落成任何静态服务器
都能直接伺候的文件。草稿不出现,和 kite 的私密语义一致。

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
  kiters-render   comrak(GFM)+ minijinja 内置布局
  kiters-build    计划与预渲染编排,落盘
apps/
  kiters-cli      kiters build
tests/fixture/    中文示例站(1 发布 + 1 草稿 + 1 页面)
```

## 路线

- [x] M1 预渲染内核(本仓库现状)
- [ ] M2 serve:ArcSwap 快照换血 + watch 增量 + 压缩协商
- [ ] M3 wasmtime + WIT 组件插件
- [ ] M4 Studio admin API
- [ ] M5 主题层 + Go-template 迁移器

设计全案见 kite 仓库 `kiters` 分支 `docs/kiters/LLM_HANDOFF.md`。

## 测试

```
cargo test        # 5 项:整站构建、CLI、钩子改写/否决/报错具名/事件载荷
```
