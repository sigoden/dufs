# Dufs

[![CI](https://github.com/sigoden/dufs/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/dufs/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/dufs.svg)](https://crates.io/crates/dufs)
[![Docker Pulls](https://img.shields.io/docker/pulls/sigoden/dufs)](https://hub.docker.com/r/sigoden/dufs)

Dufs 是一个特色鲜明的实用文件服务器，支持静态文件服务、上传、搜索、访问控制、WebDAV 等功能。

![demo](https://user-images.githubusercontent.com/4012553/220513063-ff0f186b-ac54-4682-9af4-47a9781dee0d.png)

> 本仓库是 [sigoden/dufs](https://github.com/sigoden/dufs) 的分支，在其基础上增加了多语言支持。
> 默认语言仍为英文，使用 `--lang zh-CN` 或配置文件中的 `lang: zh-CN` 即可切换为简体中文。

## 功能特性

- 提供静态文件服务
- 将文件夹下载为 zip 压缩包
- 上传文件和文件夹（拖拽上传）
- 创建 / 编辑 / 搜索文件
- 断点续传上传 / 下载
- 访问控制
- 支持 https
- 支持 webdav
- 便于配合 curl 使用

## 安装

### 通过 cargo 安装

```
cargo install dufs
```

### 通过 docker 运行

```
docker run -v `pwd`:/data -p 5000:5000 --rm sigoden/dufs /data -A
```

### 通过 [Homebrew](https://brew.sh) 安装

```
brew install dufs
```

### macOS、Linux、Windows 二进制包

从 [Github Releases](https://github.com/sigoden/dufs/releases) 下载，解压后将 dufs 加入 $PATH 即可。

## 命令行

```
Dufs is a distinctive utility file server - https://github.com/sigoden/dufs

Usage: dufs [OPTIONS] [serve-path]

Arguments:
  [serve-path]  Specific path to serve [default: .]

Options:
  -c, --config <file>        Specify configuration file
  -b, --bind <addrs>         Specify bind address or unix socket
  -p, --port <port>          Specify port to listen on [default: 5000]
      --path-prefix <path>   Specify a path prefix
      --lang <lang>          Specify the default language, e.g. en, zh-CN, auto
      --hidden <value>       Hide paths from directory listings, e.g. tmp,*.log,*.lock
  -a, --auth <rules>         Add auth roles, e.g. user:pass@/dir1:rw,/dir2
  -A, --allow-all            Allow all operations
      --allow-upload         Allow upload files/folders
      --allow-delete         Allow delete files/folders
      --allow-search         Allow search files/folders
      --allow-symlink        Allow symlink to files/folders outside root directory
      --allow-archive        Allow download folders as archive file
      --allow-hash           Allow ?hash query to get file sha256 hash
      --enable-cors          Enable CORS, sets `Access-Control-Allow-Origin: *`
      --render-index         Serve index.html when requesting a directory, returns 404 if not found index.html
      --render-try-index     Serve index.html when requesting a directory, returns directory listing if not found index.html
      --render-spa           Serve SPA(Single Page Application)
      --assets <path>        Set the path to the assets directory for overriding the built-in assets
      --log-format <format>  Customize http log format
      --log-file <file>      Specify the file to save logs to, other than stdout/stderr
      --compress <level>     Set zip compress level [default: low] [possible values: none, low, medium, high]
      --completions <shell>  Print shell completion script for <shell> [possible values: bash, elvish, fish, powershell, zsh]
      --tls-cert <path>      Path to an SSL/TLS certificate to serve with HTTPS
      --tls-key <path>       Path to the SSL/TLS certificate's private key
  -h, --help                 Print help
  -V, --version              Print version
```

## 使用示例

以只读方式提供当前目录

```
dufs
```

允许上传、删除、搜索、创建、编辑等所有操作

```
dufs -A
```

只允许上传

```
dufs --allow-upload
```

提供指定目录

```
dufs Downloads
```

提供单个文件

```
dufs linux-distro.iso
```

提供单页应用（如 react/vue）

```
dufs --render-spa
```

提供包含 index.html 的静态站点

```
dufs --render-index
```

设置用户名密码

```
dufs -a admin:123@/:rw
```

监听指定 host:ip

```
dufs -b 127.0.0.1 -p 80
```

监听 unix socket

```
dufs -b /tmp/dufs.socket
```

使用 https

```
dufs --tls-cert my.crt --tls-key my.key
```

使用简体中文界面

```
dufs --lang zh-CN
```

## API

上传文件

```sh
curl -T path-to-file http://127.0.0.1:5000/new-path/path-to-file
```

下载文件

```sh
curl http://127.0.0.1:5000/path-to-file           # 下载文件
curl http://127.0.0.1:5000/path-to-file?hash      # 获取文件的 sha256 哈希
```

将文件夹下载为 zip 压缩包

```sh
curl -o path-to-folder.zip http://127.0.0.1:5000/path-to-folder?zip
```

删除文件 / 文件夹

```sh
curl -X DELETE http://127.0.0.1:5000/path-to-file-or-folder
```

创建目录

```sh
curl -X MKCOL http://127.0.0.1:5000/path-to-folder
```

将文件 / 文件夹移动到新路径

```sh
curl -X MOVE http://127.0.0.1:5000/path -H "Destination: http://127.0.0.1:5000/new-path"
```

列出 / 搜索目录内容

```sh
curl http://127.0.0.1:5000?q=Dockerfile           # 搜索文件，类似 `find -name Dockerfile`
curl http://127.0.0.1:5000?simple                 # 仅输出文件名，类似 `ls -1`
curl http://127.0.0.1:5000?json                   # 以 json 格式输出路径
```

带认证访问（basic 与 digest 认证均支持）

```sh
curl http://127.0.0.1:5000/file --user user:pass                 # basic 认证
curl http://127.0.0.1:5000/file --user user:pass --digest        # digest 认证
```

断点续传下载

```sh
curl -C- -o file http://127.0.0.1:5000/file
```

断点续传上传

```sh
upload_offset=$(curl -I -s http://127.0.0.1:5000/file | tr -d '\r' | sed -n 's/content-length: //p')
dd skip=$upload_offset if=file status=none ibs=1 | \
  curl -X PATCH -H "X-Update-Range: append" --data-binary @- http://127.0.0.1:5000/file
```

健康检查

```sh
curl http://127.0.0.1:5000/__dufs__/health
```

<details>
<summary><h2>高级用法</h2></summary>

### 访问控制

Dufs 支持基于账号的访问控制，可以通过 `--auth`/`-a` 控制谁可以在哪些路径上做什么操作。

```
dufs -a admin:admin@/:rw -a guest:guest@/
dufs -a user:pass@/:rw,/dir1 -a @/
```

1. 使用 `@` 分隔账号和路径，不填账号表示匿名用户。
2. 使用 `:` 分隔账号的用户名和密码。
3. 使用 `,` 分隔多个路径。
4. 使用路径后缀 `:rw`/`:ro` 设置权限：`读写`/`只读`，`:ro` 可以省略。

- `-a admin:admin@/:rw`：`admin` 对所有路径拥有完整权限。
- `-a guest:guest@/`：`guest` 对所有路径只有只读权限。
- `-a user:pass@/:rw,/dir1`：`user` 对 `/*` 拥有读写权限，对 `/dir1/*` 拥有只读权限。
- `-a @/`：所有路径公开访问，任何人都可以查看 / 下载。

**认证权限受 dufs 全局权限限制。** 如果 dufs 没有通过 `--allow-upload` 开启上传权限，那么即使账号被授予了 `读写`(`:rw`) 权限，也不会拥有上传权限。

#### 哈希密码

DUFS 支持使用 sha-512 哈希密码。

生成哈希密码：

```sh
$ openssl passwd -6 123456 # 或 `mkpasswd -m sha-512 123456`
$6$tWMB51u6Kb2ui3wd$5gVHP92V9kZcMwQeKTjyTRgySsYJu471Jb1I6iHQ8iZ6s07GgCIO69KcPBRuwPE5tDq05xMAzye0NxVKuJdYs/
```

使用哈希密码：

```sh
dufs -a 'admin:$6$tWMB51u6Kb2ui3wd$5gVHP92V9kZcMwQeKTjyTRgySsYJu471Jb1I6iHQ8iZ6s07GgCIO69KcPBRuwPE5tDq05xMAzye0NxVKuJdYs/@/:rw'
```
> 哈希密码中包含 `$6`，在部分 shell 中会被当作变量展开，因此必须使用**单引号**包裹。

关于哈希密码的两点说明：

1. Dufs 只支持 sha-512 哈希密码，请确保密码字符串以 `$6$` 开头。
2. Digest 认证无法配合哈希密码正常工作。


### 隐藏路径

Dufs 支持通过 `--hidden <glob>,...` 隐藏目录列表中的路径。

```
dufs --hidden .git,.DS_Store,tmp
```

> `--hidden` 中的 glob 只匹配文件名和目录名，而不是完整路径，因此 `--hidden dir1/file` 是无效的。

```sh
dufs --hidden '.*'                          # 隐藏所有隐藏文件
dufs --hidden '*/'                          # 隐藏所有文件夹
dufs --hidden '*.log,*.lock'                # 按扩展名隐藏
dufs --hidden '*.log' --hidden '*.lock'
```

### 日志格式

Dufs 支持通过 `--log-format` 自定义 http 日志格式。

日志格式支持以下变量。

| 变量         | 说明                                                      |
| ------------ | --------------------------------------------------------- |
| $remote_addr | 客户端地址                                                |
| $remote_user | 认证时提供的用户名                                        |
| $request     | 完整的原始请求行                                          |
| $status      | 响应状态码                                                |
| $http_       | 任意请求头字段，例如 $http_user_agent、$http_referer      |


默认日志格式为 `'$time_iso8601 $log_level - $remote_addr "$request" $status`。
```
2022-08-06T06:59:31+08:00 INFO - 127.0.0.1 "GET /" 200
```

同时支持 json 日志格式。
```
dufs --log-format '{"time":"$time_local","addr":"$remote_addr","uri":"$request_uri", "method":"$request_method","status":$status}'

{"time":"2022-08-06T06:59:31+08:00","addr":"127.0.0.1","uri":"/", "method":"GET","status":200}
```

关闭 http 日志
```
dufs --log-format=''
```

记录 user-agent
```
dufs --log-format '$remote_addr "$request" $status $http_user_agent'
```
```
2022-08-06T06:53:55+08:00 INFO - 127.0.0.1 "GET /" 200 Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/104.0.0.0 Safari/537.36
```

记录 remote-user
```
dufs --log-format '$remote_addr $remote_user "$request" $status' -a /@admin:admin -a /folder1@user1:pass1
```
```
2022-08-06T07:04:37+08:00 INFO - 127.0.0.1 admin "GET /" 200
```

## 环境变量

所有选项都可以通过 `DUFS_` 前缀的环境变量设置。

```
[serve-path]                DUFS_SERVE_PATH="."
    --config <file>         DUFS_CONFIG=config.yaml
-b, --bind <addrs>          DUFS_BIND=0.0.0.0
-p, --port <port>           DUFS_PORT=5000
    --path-prefix <path>    DUFS_PATH_PREFIX=/dufs
    --lang <lang>           DUFS_LANG=zh-CN
    --hidden <value>        DUFS_HIDDEN=tmp,*.log,*.lock
-a, --auth <rules>          DUFS_AUTH="admin:admin@/:rw|@/" 
-A, --allow-all             DUFS_ALLOW_ALL=true
    --allow-upload          DUFS_ALLOW_UPLOAD=true
    --allow-delete          DUFS_ALLOW_DELETE=true
    --allow-search          DUFS_ALLOW_SEARCH=true
    --allow-symlink         DUFS_ALLOW_SYMLINK=true
    --allow-archive         DUFS_ALLOW_ARCHIVE=true
    --allow-hash            DUFS_ALLOW_HASH=true
    --enable-cors           DUFS_ENABLE_CORS=true
    --render-index          DUFS_RENDER_INDEX=true
    --render-try-index      DUFS_RENDER_TRY_INDEX=true
    --render-spa            DUFS_RENDER_SPA=true
    --assets <path>         DUFS_ASSETS=./assets
    --log-format <format>   DUFS_LOG_FORMAT=""
    --log-file <file>       DUFS_LOG_FILE=./dufs.log
    --compress <compress>   DUFS_COMPRESS=low
    --tls-cert <path>       DUFS_TLS_CERT=cert.pem
    --tls-key <path>        DUFS_TLS_KEY=key.pem
```

## 配置文件

通过 `--config <path-to-config.yaml>` 指定并使用配置文件。

可配置项如下：

```yaml
serve-path: '.'
bind: 0.0.0.0
port: 5000
path-prefix: /dufs
lang: zh-CN
hidden:
  - tmp
  - '*.log'
  - '*.lock'
auth:
  - admin:admin@/:rw
  - user:pass@/src:rw,/share
  - '@/'  # According to the YAML spec, quoting is required.
allow-all: false
allow-upload: true
allow-delete: true
allow-search: true
allow-symlink: true
allow-archive: true
allow-hash: true
enable-cors: true
render-index: true
render-try-index: true
render-spa: true
assets: ./assets/
log-format: '$remote_addr "$request" $status $http_user_agent'
log-file: ./dufs.log
compress: low
tls-cert: tests/data/cert.pem
tls-key: tests/data/key_pkcs1.pem
```

### 语言

Dufs 支持多语言，默认使用 `en`。

```
dufs --lang zh-CN
```

默认语言也可以通过配置文件设置。

```yaml
lang: zh-CN
```

请求的语言按以下顺序确定：`lang` 查询参数、`dufs_lang` cookie、`Accept-Language` 请求头，最后是默认语言。使用 `auto` 则始终跟随 `Accept-Language` 请求头。

```
dufs --lang auto
```

网页界面提供了语言切换按钮，所选语言保存在 `dufs_lang` cookie 中。

使用 docker 部署时，可以挂载配置文件来指定默认语言：

```sh
docker run -v `pwd`:/data -v `pwd`/config.yaml:/dufs/config.yaml -p 5000:5000 --rm sigoden/dufs /data -A -c /dufs/config.yaml
```

### 定制界面

Dufs 支持使用自定义资源替换内置界面。

```
dufs --assets my-assets-dir/
```

> 如果只需要对当前界面做小幅调整，可以复制 dufs 的 [assets](https://github.com/sigoden/dufs/tree/main/assets) 目录后自行修改。当前界面不依赖任何框架，只使用原生 HTML/JS/CSS，只要具备基本的 web 开发知识，修改起来并不困难。

自定义资源目录必须包含 `index.html` 文件。

`index.html` 中可以使用以下占位符获取内部数据。

- `__INDEX_DATA__`：目录列表数据
- `__ASSETS_PREFIX__`：资源 url 前缀
- `__PATH_PREFIX__`：路径前缀

> 同时支持自定义 404.html 页面。

以下是一些第三方定制界面项目：

- https://github.com/TransparentLC/dufs-material-assets
- https://github.com/cercky/dufs_web
- https://github.com/52funny/dufs-tabler-web
- https://github.com/mikigo/dufs-shadcn-assets

</details>

## 许可证

Copyright (c) 2022-2024 dufs-developers.

dufs 可依据 MIT 许可证或 Apache 许可证 2.0 使用，由你自行选择。

详见 LICENSE-APACHE 和 LICENSE-MIT 文件。
