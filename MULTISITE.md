如果你准备 **hard fork Lemmy，并接受以后长期自己维护**，那我反而认为方案 B 是可做的，而且可以比“尽量兼容上游、少改代码”的思路设计得干净很多。

关键是不要把它理解成“给 Lemmy 增加多域名”，而是直接把核心抽象从：

```text
Lemmy instance
    = 一个 LocalSite
    = 一个 hostname
    = 一个 federation identity
```

改成：

```text
Lemmy installation
    ├── LocalSite A = a.example.com
    ├── LocalSite B = b.example.com
    ├── LocalSite C = c.example.com
    └── ...
```

也就是说，**installation 与 instance 解耦**。每个 `LocalSite` 才是 Fediverse 意义上的 instance。

这在 hard fork 条件下，比我上一条给出的“尽量顺着现有 Lemmy 架构补 tenant_id”更值得做。

Lemmy 官方当前仍然是“一台 Lemmy server 有自己的站点级管理员和 moderation policy”的单实例语义，配置中 hostname 也是实例级配置。[GitHub](https://github.com/lemmynet/lemmy?utm_source=chatgpt.com)

## 我建议的核心数据模型

不要额外创造一个很重的 `tenant` 表。

直接把：

```text
local_site
```

提升为 tenant 根对象。

大致：

```sql
local_site
----------
id
site_id
hostname
tls_enabled
default_language_id
registration_mode
federation_enabled
...
```

而：

```text
site
```

继续保持 ActivityPub `Site` actor。

于是：

```text
LocalSite
   │
   ├── hostname = a.example.com
   │
   └── site_id
         ↓
       Site
       ap_id = https://a.example.com/
       inbox = https://a.example.com/inbox
       public/private key
```

这其实非常契合 Lemmy 已有的数据结构，因为 `local_site` 本来就通过 `site_id` 指向 `site`；历史 API 返回里也明确是两层对象。[GitHub](https://github.com/LemmyNet/lemmy/issues/2748?utm_source=chatgpt.com)

然后真正需要加 tenant 归属的是几个“本地身份根对象”。

我建议至少：

```sql
local_user.local_site_id
person.local_site_id NULL
community.local_site_id NULL
```

注意我这里建议 `Person` / `Community`：

```text
remote object:
local_site_id = NULL

local object:
local_site_id = N
```

而不是给所有 federated 对象强制塞 tenant。

例如：

```text
person

id   name    local_site_id     ap_id
1    alice   1                 https://a.example/u/alice
2    alice   2                 https://b.example/u/alice
3    alice   NULL              https://lemmy.world/u/alice
```

这非常重要。

因为 **远程 Person 不属于某一个 tenant**。

同一个：

```text
https://lemmy.world/u/alice
```

完全可以同时被 A、B 两个本地站看到。

如果你把每个远程 actor 都复制一份：

```text
remote alice × tenant A
remote alice × tenant B
```

数据库和 federation 会迅速变得很丑。

所以我推荐：

> 本地 actor 是 tenant-scoped；远程 actor 是 installation-global。

这是整个 fork 最重要的架构决策之一。

---

## RequestContext 应该彻底改

不要满项目到处写：

```rust
LocalSite::read(...)
```

或者：

```rust
Settings::get().hostname
```

而应该在 HTTP 最外层一次性解析：

```rust
pub struct TenantContext {
    pub local_site: LocalSite,
    pub site: Site,
    pub hostname: String,
}
```

再放进：

```rust
LemmyContext
```

但这里又有一个问题：

当前 `LemmyContext` 通常是应用级共享 context。

所以我更倾向：

```rust
pub struct LemmyContext {
    pool: DbPool,
    settings: Settings,
    federation: FederationRuntime,
    ...
}

pub struct RequestContext {
    app: LemmyContext,
    tenant: LocalSiteView,
}
```

HTTP：

```text
request
   │
   ▼
extract Host
   │
   ▼
LocalSite::read_by_hostname()
   │
   ▼
RequestContext {
    app,
    tenant
}
   │
   ├── API
   ├── auth
   ├── WebFinger
   └── ActivityPub
```

这样不会让“当前 tenant”变成 thread-local/global 变量。

**千万别做类似：**

```rust
CURRENT_SITE.with(...)
```

或者全局：

```rust
Atomic<LocalSiteId>
```

并发 federation 一跑一定出问题。

---

# hostname 应该从 Settings 中拿掉

这是 hard fork 时我会很早就做的事情。

目前配置仍然围绕一个：

```text
hostname: "example.com"
```

构建。官方 ansible 示例也仍然如此。[GitHub](https://github.com/LemmyNet/lemmy-ansible/blob/main/examples/config.hjson?utm_source=chatgpt.com)

hard fork 后我会把配置分成：

```text
安装级配置
数据库
pictrs
SMTP transport
监听地址
worker 数量
日志
对象存储
```

以及：

```text
站点级配置
hostname
站名
SMTP sender
注册策略
federation policy
管理员
theme
site icon
语言
```

例如安装配置只剩：

```hjson
{
  bind: "0.0.0.0"
  port: 8536

  database: {
    ...
  }

  pictrs: {
    ...
  }

  email: {
    smtp_server: ...
  }
}
```

站点全部进入 DB：

```text
local_site
  hostname = foo.example

local_site
  hostname = bar.example
```

这样你以后：

```text
新增站点
```

不需要：

```text
改 lemmy.hjson
重启 Lemmy
```

只需要 API：

```http
POST /api/v4/admin/sites
```

甚至可以做到即时 provision。

---

# Host routing 是第一层

我会建立：

```rust
async fn tenant_middleware(req) {
    let host = canonicalize_host(req.headers().host)?;

    let site = tenant_cache
        .get(host)
        .or_else(|| LocalSite::read_by_hostname(host));

    req.extensions_mut().insert(site);
}
```

缓存大概：

```text
HashMap<hostname, Arc<LocalSiteView>>
```

生命周期：

```text
site create / edit / delete
        ↓
invalidate hostname cache
```

不要每请求查一次 PostgreSQL。

这样 API：

```text
https://a.example.com/api/v3/site
```

自动得到 A。

而：

```text
https://b.example.com/api/v3/site
```

自动得到 B。

API 根本不应该允许客户端传：

```json
{
  "site_id": 123
}
```

选择 tenant。

**tenant 必须来自 Host。**

否则会出现大量 cross-site auth 漏洞。

---

# Auth 是第二个大工程

这里需要先决定一个产品语义：

### 模式 1：账号完全站点隔离

例如：

```text
alice@a.example
alice@b.example
```

是两个不同账号，两个密码。

这是最接近原 Lemmy/Fediverse 语义，也最容易做对。

我推荐这个。

此时登录：

```http
POST https://a.example/api/v3/user/login
```

SQL 必须：

```sql
WHERE
  local_site_id = A
AND
  username = 'alice'
```

JWT 也必须加入：

```json
{
  "sub": 123,
  "local_site_id": 42
}
```

然后请求：

```text
JWT site=A
Host site=B
```

直接：

```http
403
```

这非常重要。

---

### 模式 2：一个 installation account 横跨多个站

比如一个用户：

```text
alice
```

同时在：

```text
a.example
b.example
c.example
```

有 identity。

这个就复杂多了。

你实际上会需要额外：

```text
account
   ├── LocalUser A → Person A
   └── LocalUser B → Person B
```

我不建议第一版做。

Fediverse Identity 天然是：

```text
username@domain
```

所以让认证也跟 site 隔离，整体会干净很多。

---

# 数据库 unique constraint 是必须系统梳理的

这是 hard fork 最适合“一刀切”的地方。

目前 Lemmy 历史上已经有一些 singleton/local-site 初始化相关的唯一约束，例如过去初始化 `LocalSite` 时可以撞到：

```text
local_user_email_key
local_user_person_id_key
```

等约束。[GitHub](https://github.com/LemmyNet/lemmy/issues/3006?utm_source=chatgpt.com)

你要重新审查所有：

```sql
UNIQUE(...)
```

特别是：

```text
person.name
community.name
local_user.email
site
local_site
admin
language preference
oauth provider
registration application
captcha/rate-limit related data
```

目标大概应该变成：

```sql
UNIQUE(local_site_id, lower(name))
```

而不是：

```sql
UNIQUE(lower(name))
```

但这里有个细节：

对于 `person`：

```text
remote:
ap_id 全局唯一

local:
(local_site_id, name) 唯一
```

所以我可能会采用 partial unique indexes：

```sql
CREATE UNIQUE INDEX person_local_name_unique
ON person(local_site_id, lower(name))
WHERE local_site_id IS NOT NULL;
```

以及：

```sql
CREATE UNIQUE INDEX person_ap_id_unique
ON person(ap_id);
```

Community 同理。

这比把 instance_id 滥用成 tenant ID 清晰得多。

---

# Federation 层建议不要按请求 Host 工作

这是另一个非常重要的点。

收到请求时：

```text
Host → tenant
```

没有问题。

但是**发送 federation 时没有 HTTP inbound Host 可以依赖**。

所以 outbound activity 必须自己携带 origin：

```rust
struct SendActivity {
    local_site_id: LocalSiteId,
    actor_id: PersonId,
    activity: ...
}
```

最终 worker：

```text
SendActivity
    ↓
local_site_id
    ↓
LocalSite + Site
    ↓
domain / policy
    ↓
Actor
    ↓
private key
    ↓
HTTP Signature
```

换句话说：

> federation queue 中的每一项，都必须能确定它属于哪个 local site。

不要在 worker 中做：

```rust
Settings::get().hostname
```

这一步如果架构做干净，后面 multi-site federation 其实没有想象中那么可怕。

---

# Inbox 需要分成“谁收到”与“哪个站收到”

比如：

```text
POST https://a.example.com/inbox
```

你首先通过 Host 知道：

```text
tenant=A
```

之后才允许：

```text
A 的 person/community/site actor
```

成为 recipient。

例如某个 Activity 指向：

```text
https://b.example.com/c/foo
```

却发到：

```text
a.example.com/inbox
```

我会直接拒绝或者至少不按 local delivery 处理。

否则容易造成 tenant 穿透。

---

# WebFinger 其实会变得很漂亮

request：

```text
/.well-known/webfinger
resource=acct:alice@foo.example
```

解析：

```text
domain=foo.example
       ↓
LocalSite::by_hostname()
       ↓
Person::local_by_name(site_id, "alice")
```

然后：

```json
{
  "subject": "acct:alice@foo.example",
  "links": [
    {
      "rel": "self",
      "href": "https://foo.example/u/alice"
    }
  ]
}
```

这里甚至不必完全相信 HTTP Host。

因为 resource 自己已经带 domain。

但建议验证：

```text
resource.domain == request tenant hostname
```

避免奇怪的 cross-host 行为。

---

# 一个很容易忽略的问题：同一安装内站点之间怎么 federation？

假设：

```text
a.example
b.example
```

都属于一个 Lemmy installation。

然后：

```text
!foo@a.example
```

被：

```text
bob@b.example
```

订阅。

有两个选择。

### 做真正 federation

即：

```text
B
 ↓ HTTP ActivityPub
A
```

即使它们在同一个进程。

优点：

```text
语义100%与外部 federation 一致
测试容易
不会有特殊路径
```

缺点：

```text
浪费一次 HTTP
```

我建议 **第一版就这么干**。

千万别一开始做：

```rust
if target_site_same_installation {
    directly_insert_follow();
}
```

这种优化。

后面 Create/Delete/Announce/Undo/Block/Follow 全都要复制一套 local fast path，非常容易语义漂移。

等稳定以后再做：

```text
internal federation transport
```

但仍然让它走相同 Activity pipeline。

---

# `instance` 表最好不要等同于 local_site

Lemmy 有 `instance` 概念。

我建议保持：

```text
instance = federation domain
```

所以：

```text
a.example → instance A
b.example → instance B
lemmy.world → instance C
```

其中：

```text
instance A/B
```

只不过恰好：

```text
is local to this installation
```

因此可以考虑：

```sql
local_site.instance_id
```

或者仍通过：

```text
LocalSite
 → Site
 → instance_id
```

关联。

不要增加：

```text
instance.is_local BOOLEAN
```

除非确实能简化大量代码。

因为更精确的定义应该是：

```text
EXISTS local_site WHERE site.instance_id = instance.id
```

---

# 管理员模型要重新定义

当前 Lemmy 的描述本身就是 site-wide admins。[GitHub](https://github.com/lemmynet/lemmy?utm_source=chatgpt.com)

multi-site 后：

```text
admin
```

必须变成：

```text
site admin
```

例如：

```sql
local_user.admin
```

其实可以保留，如果 `local_user` 已 tenant-scoped：

```text
alice@A admin=true
alice@B admin=false
```

天然解决。

但你很可能还会需要一个新的：

```text
installation_admin
```

负责：

```text
创建站点
删除站点
查看所有 tenant
配置 SMTP
维护 pictrs
数据库 maintenance
```

我建议它**不要作为 Fediverse Person**。

单独：

```sql
installation_admin
```

或者后台 auth。

这是比较干净的权限边界：

```text
Installation Admin
        │
        ├── Site A Admin
        ├── Site B Admin
        └── Site C Admin
```

不要让：

```text
admin@a.example
```

默认拥有 B 的权限。

---

# 封禁也需要明确 scope

这块会比想象中更重要。

例如：

```text
ban Person X
```

是：

```text
只在 site A ban
```

还是：

```text
整个 installation ban
```

第一版建议所有现有 Lemmy moderation 都保持：

```text
LocalSite scoped
```

然后另外增加安装级：

```text
global denylist
```

仅用于 spam/abuse。

类似 federation：

```text
A blocks evil.example
B does not block evil.example
```

应该能够成立。

所以当前的 federation allow/block policy 都必须拿：

```text
local_site_id
```

---

# 邮件和图片反而不是大问题

SMTP transport 可以全局：

```text
smtp.example.net
```

但：

```text
From:
Reply-To:
site name:
base URL:
```

是 per-site。

比如：

```text
a.example → noreply@a.example
b.example → noreply@b.example
```

官方配置现在把 SMTP sender 和单一 domain 放在同一个实例配置里，所以 hard fork 后把 sender metadata 下沉到 LocalSite 是自然的。[GitHub](https://github.com/LemmyNet/lemmy-ansible/blob/main/examples/config.hjson?utm_source=chatgpt.com)

Pictrs 我认为完全可以全局共享。

第一版不用隔离。

---

# UI 可以后做

Lemmy UI 本身只要：

```text
browser Host=A
        ↓
GET /api/v3/site
        ↓
返回 A 的 SiteView
```

很多功能天然就会工作。

所以 backend 做好之后，UI 修改量未必特别大。

真正需要调整的是：

```text
SSR 时 backend URL
canonical URL
OpenGraph URL
cookie domain
site metadata
favicon
```

如果你继续使用独立 `lemmy-ui`。

---

# Cookie 有个很好用的天然隔离

千万不要设置：

```text
Domain=.example.com
```

如果：

```text
a.example.com
b.example.com
```

都是站点。

使用 host-only cookie：

```text
a.example.com cookie
b.example.com cookie
```

自然隔离。

再配合 JWT：

```text
local_site_id
```

等于双保险。

---

# 我会把整个 hard fork 分成 6 个阶段

如果是我实际动这个 fork，我不会一上来改 federation。

### Phase 1 — Tenant foundation

先做到：

```text
local_site N rows
hostname → local_site
RequestContext
/site 根据 Host 返回不同站点
```

此时：

```text
federation 暂时关闭
```

验证：

```text
a.example
b.example
```

一进程一 DB 能正确返回两个 Site。

---

### Phase 2 — 本地身份

做：

```text
Person.local_site_id
Community.local_site_id
LocalUser.local_site_id
```

然后：

```text
注册
登录
创建社区
发帖
评论
moderation
```

全部 tenant 隔离。

测试最关键：

```text
A 创建 alice
B 创建 alice
```

和：

```text
A 创建 !general
B 创建 !general
```

必须同时成立。

---

### Phase 3 — 权限隔离

系统查：

```text
admin
ban
registration
OAuth
email verification
password reset
reports
modlog
site settings
```

所有 tenant crossing。

这个阶段做完，才算真正的：

```text
multi-site Lemmy
```

即使 federation 还是关闭的。

---

### Phase 4 — ActivityPub identity

然后改：

```text
ap_id generation
WebFinger
Actor fetch
Site actor
Person actor
Community actor
inbox
outbox
HTTP Signature
```

做到：

```text
curl a.example/u/alice
curl b.example/u/alice
```

返回不同 actor。

---

### Phase 5 — outbound federation

重点改：

```text
activity queue
send task
signing
federation policy
allow/block
retry state
```

让每个任务携带：

```text
local_site_id
```

然后拿第三个外部 Lemmy 测：

```text
A ↔ C
B ↔ C
```

---

### Phase 6 — A ↔ B

最后测试同 installation：

```text
A ↔ B
```

仍然走 federation。

只有这一步跑通以后，我才会认为整个模型正确。

---

## 修改量如果 hard fork，我反而不会认为“太大”

如果要求：

> 做一个 PR 给 Lemmy upstream，而且尽量不破坏现有抽象。

那我会说工程风险很高。

但是你现在说：

> **hard fork。**

性质就不一样了。

你可以主动删除很多 singleton abstraction，而不是兼容它：

```text
Settings.hostname           → 删除
LocalSite::read singleton   → 删除
site-wide global helpers    → 改成必须传 LocalSiteId
```

甚至利用 Rust 类型系统强迫所有代码改正确。

例如我很建议造：

```rust
#[derive(Clone, Copy)]
pub struct LocalSiteId(...);

pub struct LocalSiteContext {
    pub id: LocalSiteId,
    pub hostname: Hostname,
    pub site: Site,
}
```

然后任何会生成本地 URL 的函数：

原：

```rust
fn generate_local_apub_endpoint(kind, name)
```

改成：

```rust
fn generate_local_apub_endpoint(
    site: &LocalSiteContext,
    kind: EndpointType,
    name: &str,
)
```

编译器会帮你把所有 singleton usage 一个个揪出来。

这比：

```text
grep hostname
grep LocalSite::read
然后人工判断
```

可靠很多。

---

# 甚至可以更激进：禁止“隐式本地”

这是我尤其推荐 hard fork 做的。

目前很多 Fediverse 软件内部有：

```rust
object.local
```

或者：

```text
is_local(ap_id)
```

含义其实是：

```text
belongs to THE local hostname
```

你的 fork 应该改成：

```rust
object.local_site_id: Option<LocalSiteId>
```

于是：

```rust
object.local_site_id.is_some()
```

表示：

```text
属于本 installation
```

而：

```rust
object.local_site_id == Some(request.site.id)
```

表示：

```text
属于当前 site
```

这是两个不同概念。

我甚至会把 helper 明确命名：

```rust
is_installation_local()
is_site_local(site_id)
```

**不要继续保留 `is_local()` 这种模糊 API。**

否则半年以后一定有人把这两个概念混起来。

---

# 最终的数据关系我会设计成这样

```text
                    Installation
                         │
                ┌────────┴────────┐
                │                 │
          LocalSite A       LocalSite B
          a.example         b.example
                │                 │
              Site              Site
                │                 │
           Instance A        Instance B
                │                 │
        ┌───────┴──────┐   ┌─────┴───────┐
        │              │   │             │
     Person         Community          Person
    alice@A         general@A         alice@B
        │
    LocalUser
```

而远程：

```text
                    Instance C
                   lemmy.world
                         │
             ┌───────────┴───────────┐
           Person                 Community
        remote_alice              rust
```

它们**不复制 A/B 两份**。

站点 A/B 与远程对象之间的 tenant-specific 状态：

```text
follow
subscription
block
ban
federation state
```

放关联表。

这个模型在我看来是最值得 hard fork 的方向。

---

## 工作量我会这样估

如果目标只是 MVP：

```text
2 个域名
单 DB
本地用户隔离
社区隔离
帖子评论
基本 federation
```

对于一个已经熟悉 Lemmy 代码库的 Rust 开发者，我会按：

**大约 3–6 周全职开发量** 看待。

但如果目标是：

```text
完整 API
全部 moderation
admin
OAuth
email
federation allow/block
scheduled tasks
purge
reports
modlog
rate limit
完整 migration
lemmy-ui
测试
升级策略
```

更现实是：

**2–4 个月级别。**

hard fork 后真正贵的也不是第一版，而是每次 merge upstream 时：

```text
上游新代码是不是又偷偷假设只有一个 LocalSite？
```

所以我会在 CI 中专门做一个 **双站点测试环境**：

```text
site-a.test
site-b.test
external.test
```

并规定任何 integration test 都至少验证 A/B 隔离。

---

如果你已经确定走 hard fork + 方案 B，我建议下一步不要先写代码，而是先做一次 **Lemmy 当前 `main` 的 multi-tenant 改造点审计**：我可以直接按当前 GitHub 源码，把 `Settings.hostname`、`LocalSite` singleton、`is_local`、URL 生成、WebFinger、federation worker、auth、DB unique constraints 等调用点逐类找出来，最后给你一份**具体到 crate / Rust 文件 / struct / migration 的改造清单**。这样基本就能直接作为 fork 的实施设计文档。