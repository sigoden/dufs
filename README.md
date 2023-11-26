# Dufs

[![CI](https://github.com/sigoden/dufs/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/dufs/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/dufs.svg)](https://crates.io/crates/dufs)

Dufs is a distinctive utility file server that supports static serving, uploading, searching, accessing control, webdav...

![demo](https://user-images.githubusercontent.com/4012553/220513063-ff0f186b-ac54-4682-9af4-47a9781dee0d.png)

## Features

- Serve static files
- Download folder as zip file
- Upload files and folders (Drag & Drop)
- Create/Edit/Search files
- Partial responses (Parallel/Resume download)
- Access control
- Support https
- Support webdav
- Easy to use with curl

## Install

### With cargo

```
cargo install dufs
```

### With docker

```
docker run -v `pwd`:/data -p 5000:5000 --rm -it sigoden/dufs /data -A
```

### With [Homebrew](https://brew.sh)

```
brew install dufs
```

### Binaries on macOS, Linux, Windows

Download from [Github Releases](https://github.com/sigoden/dufs/releases), unzip and add dufs to your $PATH.

## CLI

```
Dufs is a distinctive utility file server - https://github.com/sigoden/dufs

Usage: dufs [OPTIONS] [serve-path]

Arguments:
  [serve-path]  Specific path to serve [default: .]

Options:
  -c, --config <config>      Specify configuration file
  -b, --bind <addrs>         Specify bind address or unix socket
  -p, --port <port>          Specify port to listen on [default: 5000]
      --path-prefix <path>   Specify a path prefix
      --hidden <value>       Hide paths from directory listings, e.g. tmp,*.log,*.lock
  -a, --auth <rules>         Add auth roles, e.g. user:pass@/dir1:rw,/dir2
  -A, --allow-all            Allow all operations
      --allow-upload         Allow upload files/folders
      --allow-delete         Allow delete files/folders
      --allow-search         Allow search files/folders
      --allow-symlink        Allow symlink to files/folders outside root directory
      --allow-archive        Allow zip archive generation
      --enable-cors          Enable CORS, sets `Access-Control-Allow-Origin: *`
      --render-index         Serve index.html when requesting a directory, returns 404 if not found index.html
      --render-try-index     Serve index.html when requesting a directory, returns directory listing if not found index.html
      --render-spa           Serve SPA(Single Page Application)
      --assets <path>        Set the path to the assets directory for overriding the built-in assets
      --log-format <format>  Customize http log format
      --completions <shell>  Print shell completion script for <shell> [possible values: bash, elvish, fish, powershell, zsh]
      --tls-cert <path>      Path to an SSL/TLS certificate to serve with HTTPS
      --tls-key <path>       Path to the SSL/TLS certificate's private key
  -h, --help                 Print help
  -V, --version              Print version
```

## Examples

Serve current working directory in read-only mode

```
dufs
```

Allow all operations like upload/delete/search/create/edit...

```
dufs -A
```

Only allow upload operation

```
dufs --allow-upload
```

Serve a specific directory

```
dufs Downloads
```

Serve a single file

```
dufs linux-distro.iso
```

Serve a single-page application like react/vue

```
dufs --render-spa
```

Serve a static website with index.html

```
dufs --render-index
```

Require username/password

```
dufs -a admin:123@/:rw
```

Listen on specific host:ip 

```
dufs -b 127.0.0.1 -p 80
```

Listen on unix socket
```
dufs -b /tmp/dufs.socket
```

Use https

```
dufs --tls-cert my.crt --tls-key my.key
```

## API

Upload a file

```
curl -T path-to-file http://127.0.0.1:5000/new-path/path-to-file
```

Download a file
```
curl http://127.0.0.1:5000/path-to-file
```

Download a folder as zip file

```
curl -o path-to-folder.zip http://127.0.0.1:5000/path-to-folder?zip
```

Delete a file/folder

```
curl -X DELETE http://127.0.0.1:5000/path-to-file-or-folder
```

Create a directory

```
curl -X MKCOL https://127.0.0.1:5000/path-to-folder
```

Move the file/folder to the new path

```
curl -X MOVE https://127.0.0.1:5000/path -H "Destination: https://127.0.0.1:5000/new-path"
```

List/search directory contents

```
curl http://127.0.0.1:5000?q=Dockerfile           # search for files, similar to `find -name Dockerfile`
curl http://127.0.0.1:5000?simple                 # output names only, similar to `ls -1`
curl http://127.0.0.1:5000?json                   # output paths in json format
```

With authorization

```
curl http://192.168.8.10:5000/file --user user:pass                 # basic auth
curl http://192.168.8.10:5000/file --user user:pass --digest        # digest auth
```

<details>
<summary><h2>Advanced topics</h2></summary>

### Access Control

Dufs supports account based access control. You can control who can do what on which path with `--auth`/`-a`.

```
dufs -a user:pass@/path1:rw,/path2 -a user2:pass2@/path3 -a @/path4
```

1. Use `@` to separate the account and paths. No account means anonymous user.
2. Use `:` to separate the username and password of the account.
3. Use `,` to separate paths.
4. Use `:rw` suffix to indicate that the account has read-write permission on the path.

- `-a admin:amdin@/:rw`: `admin` has complete permissions for all paths.
- `-a guest:guest@/`: `guest` has read-only permissions for all paths.
- `-a user:pass@/dir1:rw,/dir2`: `user` has complete permissions for `/dir1/*`, has read-only permissions for `/dir2/`.
- `-a @/`: All paths is publicly accessible, everyone can view/download it.

> There are no restrictions on using ':' and '@' characters in a password, `user:pa:ss@1@/:rw` is valid, and the password is `pa:ss@1`.

#### Hashed Password

DUFS supports the use of sha-512 hashed password.

Create hashed password

```
$ mkpasswd  -m sha-512 -s
Password: 123456 
$6$qCAVUG7yn7t/hH4d$BWm8r5MoDywNmDP/J3V2S2a6flmKHC1IpblfoqZfuK.LtLBZ0KFXP9QIfJP8RqL8MCw4isdheoAMTuwOz.pAO/
```

Use hashed password
```
dufs \
  -a 'admin:$6$qCAVUG7yn7t/hH4d$BWm8r5MoDywNmDP/J3V2S2a6flmKHC1IpblfoqZfuK.LtLBZ0KFXP9QIfJP8RqL8MCw4isdheoAMTuwOz.pAO/@/:rw'
```

Two important things for hashed passwords:

1. Dufs only supports sha-512 hashed passwords, so ensure that the password string always starts with `$6$`.
2. Digest authentication does not function properly with hashed passwords.


### Hide Paths

Dufs supports hiding paths from directory listings via option `--hidden <glob>,...`.

```
dufs --hidden .git,.DS_Store,tmp
```

> The glob used in --hidden only matches file and directory names, not paths. So `--hidden dir1/file` is invalid.

```sh
dufs --hidden '.*'            # hidden dotfiles
dufs --hidden '*/'            # hidden all folders
dufs --hidden '*.log,*.lock'  # hidden by exts
```

### Log Format

Dufs supports customize http log format with option `--log-format`.

The log format can use following variables.

| variable     | description                                                               |
| ------------ | ------------------------------------------------------------------------- |
| $remote_addr | client address                                                            |
| $remote_user | user name supplied with authentication                                    |
| $request     | full original request line                                                |
| $status      | response status                                                           |
| $http_       | arbitrary request header field. examples: $http_user_agent, $http_referer |


The default log format is `'$remote_addr "$request" $status'`.
```
2022-08-06T06:59:31+08:00 INFO - 127.0.0.1 "GET /" 200
```

Disable http log
```
dufs --log-format=''
```

Log user-agent
```
dufs --log-format '$remote_addr "$request" $status $http_user_agent'
```
```
2022-08-06T06:53:55+08:00 INFO - 127.0.0.1 "GET /" 200 Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/104.0.0.0 Safari/537.36
```

Log remote-user
```
dufs --log-format '$remote_addr $remote_user "$request" $status' -a /@admin:admin -a /folder1@user1:pass1
```
```
2022-08-06T07:04:37+08:00 INFO - 127.0.0.1 admin "GET /" 200
```

## Environment variables

All options can be set using environment variables prefixed with `DUFS_`.

```
[serve-path]                DUFS_SERVE_PATH="."
    --config <path>         DUFS_CONFIG=config.yaml
-b, --bind <addrs>          DUFS_BIND=0.0.0.0
-p, --port <port>           DUFS_PORT=5000
    --path-prefix <path>    DUFS_PATH_PREFIX=/path
    --hidden <value>        DUFS_HIDDEN=*.log
-a, --auth <rules>          DUFS_AUTH="admin:admin@/:rw|@/" 
-A, --allow-all             DUFS_ALLOW_ALL=true
    --allow-upload          DUFS_ALLOW_UPLOAD=true
    --allow-delete          DUFS_ALLOW_DELETE=true
    --allow-search          DUFS_ALLOW_SEARCH=true
    --allow-symlink         DUFS_ALLOW_SYMLINK=true
    --allow-archive         DUFS_ALLOW_ARCHIVE=true
    --enable-cors           DUFS_ENABLE_CORS=true
    --render-index          DUFS_RENDER_INDEX=true
    --render-try-index      DUFS_RENDER_TRY_INDEX=true
    --render-spa            DUFS_RENDER_SPA=true
    --assets <path>         DUFS_ASSETS=/assets
    --log-format <format>   DUFS_LOG_FORMAT=""
    --tls-cert <path>       DUFS_TLS_CERT=cert.pem
    --tls-key <path>        DUFS_TLS_KEY=key.pem
```

## Configuration File

You can specify and use the configuration file by selecting the option `--config <path-to-config.yaml>`.

The following are the configuration items:

```yaml
serve-path: '.'
bind:
  - 192.168.8.10
port: 5000
path-prefix: /dufs
hidden:
  - tmp
  - '*.log'
  - '*.lock'
auth:
  - admin:admin@/:rw
  - user:pass@/src:rw,/share
allow-all: false
allow-upload: true
allow-delete: true
allow-search: true
allow-symlink: true
allow-archive: true
enable-cors: true
render-index: true
render-try-index: true
render-spa: true
assets: ./assets/
log-format: '$remote_addr "$request" $status $http_user_agent'
tls-cert: tests/data/cert.pem
tls-key: tests/data/key_pkcs1.pem
```

### Customize UI

Dufs allows users to customize the UI with your own assets.

```
dufs --assets my-assets-dir/
```

Your assets folder must contains a `index.html` file.

`index.html` can use the following placeholder variables to retrieve internal data.

- `__INDEX_DATA__`: directory listing data
- `__ASSETS_PREFIX__`: assets url prefix

</details>

## License

Copyright (c) 2022 dufs-developers.

dufs is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.
