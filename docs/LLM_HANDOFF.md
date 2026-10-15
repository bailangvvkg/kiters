# kiters 移交文档(LLM_HANDOFF)

> 这是 kiters 的自包含交接文档。前史(为何重写、五签名设计、crate 选型、M1 实录)在
> kite 仓库 `kiters` 分支 `docs/kiters/LLM_HANDOFF.md`,仍有效。本文件接续 M2 起。

## 0. 现状一句话

四里程碑走完两个:M1 预渲染内核、M2 serve 全家、M3 wasm 插件,全部 CI 双绿
(test+image),镜像 `ghcr.io/bailangvvkg/kiters:latest`。**不接管生产流量**:
halo 主站、kite(Go)备胎、kiters 只验收路线。

## 1. 部署姿态(用户钦定,2026-10)

- 不接管 kite.zipimg.cn 流量。
- 生产链路不动:EdgeOne → Lucky → kite:1717(Go 版)。
- kiters = 长线重写,验收靠 CI + 镜像,**服务器不参与编译**(3865U 太弱,用户否决)。
- 工作流:本地纯编辑 → push → Actions(test+image)→ ghcr。本地无 Rust 工具链。

## 2. M2 实录(已完成,35ba8a5 起)

- `crates/kiters-serve`:axum 0.8 + ArcSwap 快照。**页面按路径寻址,query 天然免疫**
  (无 per-query 状态,洪水=同一页)。响应 `x-kiters: snapshot` + `cache-control: no-store`。
- 压缩:tower-http br/zstd/gzip,`compress_when(SizeAbove::new(1024))` —— tower-http
  默认阈值是 32 字节,别踩。
- watch:notify 8,burst 去抖 150ms,定时发布走时钟臂(最近 due 时间唤醒,空闲分钟
  不重建)。**watcher 必须移进线程保活**,函数返回即 drop。
- 短代码:`{{< note|info|tip|warn 标题 >}}…{{< /kind >}}` → comrak 前折叠成引用块
  (`crates/kiters-render/src/shortcode.rs`)。不需要 unsafe HTML。kite 精确视觉对齐留 M5。
- 别名页:front matter `aliases:` → 200 页 + meta refresh,指向 item 真实地址。
- 税目页:`/tags/` 索引 + `/tags/<term>/`(term URL 百分号编码,快照键=编码形态)。
- 定时:`Status::stands(dated, now)` 单一裁决,published 恒立、scheduled 到点立、draft 永不立。
- CLI:`kiters serve --site --addr(0.0.0.0:1718) --drafts --no-watch`。

## 3. M3 实录(已完成,eff9ed1)

- **协议**:`crates/kiters-wasm/wit/kiters.wit`,world `kiters:plugin@0.1.0`。
  七导出对应七钩子;guest 必须实现全部(wit-bindgen 0.62 无默认实现),不关心的原样返回。
- **宿主**:`crates/kiters-wasm`,wasmtime 49 + wasmtime-wasi(p2)。
  引擎进程级共享;组件 `.wasm` 编译一次;实例实例化一次复用(Mutex)。
  **燃料 20 亿/调用,实例化前就要 set_fuel**(实例化也烧油)。
  WASI 最小实现:无文件、无时钟、无随机 —— 插件是变换不是进程。
- **桥**:`WasmPlugin` 实现 `kiters_core::Hook`。item 的 id=slug(core 无 id 字段)。
  `Extra` kind 是站点自有的,插件不能造(kind_from 返回 None)。
- **guest 编译**:target `wasm32-wasip2`(rustup target add),cdylib 直出组件,
  产物 `target/wasm32-wasip2/debug/<pkg_name_下划线>.wasm`。
  CI 先编 guest 再 `cargo test --workspace`(测试从 target 目录读 .wasm)。
- **样例插件** `plugins/`:stamp-wasm(on_page 戳记)、skipper-wasm(on_target 撤页)、
  linkcard-lite(on_html 裸链接→卡片,对拍 kite linkcard 的行为契约)。
- **CLI**:`--plugin <x.wasm>`(可多次)挂在 build 与 serve;插件随每次重建复用。
- wasmtime 49 坑:Error 自有类型(非 anyhow re-export);`HasData`/`WasiView` trait;
  空 linker 无需 add_to_linker(但 wasip2 guest 要 `p2::add_to_linker_sync`)。

## 4. M4 实录(admin API,已完成,a59fc42)

- 新 crate `crates/kiters-admin`:`store.rs`(ContentStore:文件树即唯一真相,`content/<kind>/<slug>/index.md`,地址 `kind:slug`,revision = 文件字节 FNV-1a 64 位 hex;WriteError:Conflict/Exists/Missing/BadName/Io)、`session.rs`(HMAC-SHA256 cookie,密钥=sha256(password),常数时间比较,12h TTL)、`lib.rs` 路由。
- 路由:POST /api/login、/api/logout、GET /api/session;GET/POST /api/contents(cursor 分页)、GET/PUT/DELETE /api/contents/{id}(PUT 必带 If-Match,缺=428,过期=409 附 current_revision;DELETE 同样 revision 门;400 当 kind 缺失/不匹配);POST /api/media(multipart)、GET /media/{*path}(immutable 缓存 + 路径穿越防护);POST /api/preview(no-store 即时渲染);GET /api/export(zip)、POST /api/import(zip,路径校验)。
- serve 集成:`app_with_admin(served, Option<(PathBuf, String)>)`;CLI `serve --admin <密码>` 或 env KITERS_ADMIN_PASSWORD。写 content/ → watch 重建 → 快照换血 = 预览闭环。
- 测试(admin_test.rs):登录门(无 cookie 401)、条目生命周期、并发写竞态(恰一 200 一 409)、预览含短代码、export→import 往返、删除 revision 门、媒体上传 + `..%2f` 穿越被拒、kind 必填 400。
- 踩坑:br 原始串不能含非 ASCII(中文用 `r#\"…\"#.as_bytes()`);hmac 0.13 要 `use hmac::digest::KeyInit`;axum 0.8 multipart。

kite admin API 契约(admin Studio React 直连):登录会话、内容 CRUD(cursor 分页、
If-Match/revision 乐观锁、409 冲突)、媒体上传、预览(no-store 语义)、导入导出。
要点(从 kite 生产事实抄):
- PUT /contents/{id} **必须带 kind 字段**(漏了 400)。
- revision 冲突 → 409,If-Match 头携带。
- 预览响应 no-store。
- kiters 侧:内容写回 content/ 目录 → watch 重建 → 快照换血,天然预览闭环。
- 形态:axum 路由挂在 serve 上(`/api/*`),认证用签名 cookie 会话。

## 5. M5 实录(主题/上色/迁移器/搜索,已完成,c079007)

- **主题系统**(kiters-render/src/layout.rs):`Layout::theme(dir)` minijinja set_loader,主题模板先答、静态内置回退。主题可接管整页:`page.html`/`list.html`(上下文 site/title/items,每 item 含 title/url/date)、`alias.html`、`notfound.html`;sitemap/rss 恒用内置。`SiteBuilder::theme(name)` 读 `themes/<name>/`;CLI build/serve 均有 `--theme`。
- **上色**(kiters-render/src/highlight.rs):syntect 5 `default-fancy`(纯 Rust regex,musl 安全),InspiredGitHub 亮色主题,扫描 `<pre><code class=\"language-x\">` 替换;无 language 或语言不识别→字节不动。CLI `--highlight`(build 与 serve 都有,ServeOptions.highlight 传进 watch 重建)。OnceLock 全进程共享一份色表。
- **迁移器**(apps/kiters-cli/src/migrate.rs):`kiters migrate-template --from a.html --to b.html`。变量与字段路径(`.Site.Title`→`site.title`;整词大写缩写 URL/RSS 全词小写)、if/else/end(栈式配对)、range→for(循环变量=集合名单数,体内裸 `.` 与 `.Title` 自动带循环名)、注释、printf 取参数;未识别动作保留原样并在结尾点名。
- **搜索索引**:构建产物 `/search.json`(plan Extra("search")),每篇站立条目 {title,url,date,text(500 字纯文)},serve 按 .json 后缀回 application/json。不上 tantivy/jieba(太重),检索交给主题端 JS。
- 踩坑:minijinja 2.x set_loader 返回 `Result<Option<String>, Error>`;minijinja html 自动转义会把 `/` 转 `&#x2F;`(自家 URL 用 `|safe`);`"<pre><code"` 前缀里 `<pre>` 自带 `>`,找开标签结尾要从前缀之后搜;ThemeSet 取主题走 `.themes["…"]`;bash heredoc 写 Rust `'
'` 会变字面换行(用 edit 工具)。

## 6. 验收基线(每里程碑)

- CI 双绿(test + image),镜像体积记录进 README。
- fixture 行为测试为主,服务器 docker 仅验收不编译。
- 每步 push 即验收,kci.py(临时目录)盯 Actions。

## 7. 仓库地标

```
crates/kiters-core     七钩子 + 类型(hooks.rs/site.rs)
crates/kiters-parse    walk + front matter
crates/kiters-render   comrak + minijinja layout + shortcode.rs
crates/kiters-build    plan.rs(七种 TargetKind)+ report.rs(build_site)
crates/kiters-serve    lib.rs(app/serve, /api/* 经 app_with_admin 挂 admin) snapshot.rs watch.rs
                      (ServeOptions{with_drafts,plugins,theme,highlight})
crates/kiters-admin    store.rs(文件树真相+revision) session.rs(HMAC cookie) lib.rs(路由)
crates/kiters-wasm     lib.rs(WasmPlugin 桥) wit/kiters.wit(协议)
plugins/               stamp-wasm / skipper-wasm / linkcard-lite
apps/kiters-cli        build + serve + migrate-template 子命令(migrate.rs 含单测)
tests/fixture          5 内容项(1 草稿 1 定时)含别名/税目/长文 + themes/demo/
```

## 8. almanac 主题(kite 的皮,kiters 穿上)

生产容器即穿此主题。kite 的 almanac 模板(Tailwind 编译产物 102K css + 21K js)
原样复用,模板移植为 minijinja:

- `themes/almanac/`:baseof.html(骨架:头部/亮暗 init/og)+ page(文章)/
  list(首页与词目)/taxonomy(分类索引)/notfound/alias + `_partials/`
  (header/footer/icon 内联 SVG macro/theme-init 亮暗记忆)。
- **minijinja 雷区**:replace 过滤器参数是位置式(`replace('-', '_')`),
  kwargs 版直接炸;map 取值 `svgs[name] | default()`,没有 `.get()`;
  模板渲染失败曾静默成空串——`Layout::say` 现在 eprintln 出错因。
- 主题 static/:**`static/` 下的内容即站点根**。build 落盘到输出目录,
  snapshot 整包读进内存,serve 按 `/almanac/...` 地址回答
  (css/js/svg/ico/png/webp/woff2 内容类型 + immutable 缓存)。
  页面自己说 no-store,`SetResponseHeaderLayer` 是 `if_not_present`。
- 主题设置:`kite.yaml` 的 `theme.settings.*` 整块进 `SiteMeta.theme_settings`
  (yaml Mapping),模板里 `site.theme_settings.web_fonts` 直读。
  `theme_settings.taxonomy` 决定导航指向哪个分类索引(本站 categories)。
- 税目跟随站点:said_tags 取 item 实有的 taxonomy(tags 优先,否则
  categories 等),term 地址两侧同源(layout 的 encode_term 与 plan 的
  term_encode 同规则:非保留字节百分号大写)。
- `markdown.unsafeHTML: true`(kite.yaml)现在由 SiteBuilder::new 自己读,
  不再要 --unsafe 标志(显式 --unsafe 仍是并集)。上色 span 由此保留。
- watch 连 `themes/` 一起看(递归):改模板不用重启容器。
- 部署命令(生产):
  ```
  docker run -d --name kiters --restart unless-stopped --network host     -v /root/kiters-site:/site -w /site     ghcr.io/bailangvvkg/kiters:latest serve --site /site --addr 0.0.0.0:1718     --highlight --admin 'kiters@zipimg2026'     --plugin /site/plugins/linkcard_lite.wasm --theme almanac
  ```
  本地改主题 → tar 传 /root/kiters-site/themes/ → watch 自动重建(不用重启)。

### 8.1 首页卡片与 studio(用户验收后返工)

- **首页卡片**照原版 card-post 画:左窄图(w-28 sm:w-52,占满卡片高)+
  右侧 min-h-36(meta 行 mono 11px / 标题 15px line-clamp-2 / 摘要 13px /
  tags 行)。`card` `card-hover` `min-h-36` `line-clamp-2` 都在编译好的
  almanac.css 里,直接用。
- **studio 重写**(studio.html 单文件,内嵌全套 CSS 变量,亮暗跟随系统):
  左侧栏列表(状态圆点/标题/id/日期,active 高亮)+ 右侧编辑区(大标题
  输入 + 元信息网格 + mono 正文 + Ctrl+S)+ 预览分栏(iframe srcdoc
  带纸感样式)+ 新建模态(不再 prompt)+ 右下 toast + 未保存警示。
  API 契约不变(login/session/contents/preview,PUT 带 If-Match)。
- **踩雷:改主题模板不生效**——minijinja 的 set_loader 结果**进程内缓存**,
  文件改了进程不重读(重启才读)。修:每次 rebuild 前
  `layout.forget_templates()`(env.clear_templates,需 &mut)。事件
  inotify 其实在 bind mount 容器里工作正常;另留 20s 时钟兜底
  (theme_stamp 比对主题树最新 mtime),防 rm+tar 整树替换丢 watch。

### 8.2 RSS 摘要化 + 零拷贝压测(wrk 实录)

- **RSS 对齐 kite**:description 从 308K 全文 HTML 改为 **300 字纯文本摘要**
  (plain_text + tasted,截断加 …);guid 用 front matter 的 halo id
  (isPermaLink=false,无 id 用路径);补 atom:link self / language /
  generator / lastBuildDate;cover → media:content。308K → 17.3K。
- **零拷贝**:Snapshot pages/assets 从 Arc<Page>.html.clone()(每请求整页
  复制)改为 **bytes::Bytes**(引用计数,Body::from(Bytes) 零拷贝)。
  workspace 加 bytes = "1"(记得 kiters-serve/Cargo.toml 也要
  bytes.workspace = true)。
- **压测**(服务器 3865U 单核 cpuset,wrk -t1 -c100 -d10s):
  | 端点 | kite(Go:1717) | kiters 改前 | kiters 改后 |
  |---|---|---|---|
  | /(首页 38K) | 13,390 r/s | 6,952 r/s | **12,220 r/s** |
  | /rss.xml | 13,626 r/s | 1,808 r/s(308K 时) | **11,690 r/s** |
  | /almanac/css(102K) | — | — | 7,977 r/s(≈790MB/s,单核内存带宽顶) |
  | RSS 内存 | 198→320 MiB(压测中) | **12.3→14.8 MiB** | 同左 |
  kite 压测完回落 215 MiB;kiters 常驻 ~13 MiB、压测全程不涨。
