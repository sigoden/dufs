# Dufs

[![CI](https://github.com/sigoden/dufs/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/dufs/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/dufs.svg)](https://crates.io/crates/dufs)

Dufs is a distinctive utility file server that supports static serving, uploading, searching, accessing control, webdav...

![demo](https://user-images.githubusercontent.com/4012553/177549931-130383ef-0480-4911-b9c2-0d9534a624b7.png)

## Features

- Serve static files
- Download folder as zip file
- Upload files and folders (Drag & Drop)
- Search files
- Partial responses (Parallel/Resume download)
- Path level access control
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

### Binaries on macOS, Linux, Windows

Download from [Github Releases](https://github.com/sigoden/dufs/releases), unzip and add dufs to your $PATH.

## CLI

```
Dufs is a distinctive utility file server - https://github.com/sigoden/dufs

USAGE:
    dufs [OPTIONS] [--] [root]

ARGS:
    <root>    Specific path to serve [default: .]

OPTIONS:
    -b, --bind <addr>...         Specify bind address
    -p, --port <port>            Specify port to listen on [default: 5000]
        --path-prefix <path>     Specify a path prefix
        --hidden <value>...      Hide paths from directory listings
    -a, --auth <rule>...         Add auth for path
        --auth-method <value>    Select auth method [default: digest] [possible values: basic, digest]
    -A, --allow-all              Allow all operations
        --allow-upload           Allow upload files/folders
        --allow-delete           Allow delete files/folders
        --allow-search           Allow search files/folders
        --allow-symlink          Allow symlink to files/folders outside root directory
        --enable-cors            Enable CORS, sets `Access-Control-Allow-Origin: *`
        --render-index           Serve index.html when requesting a directory, returns 404 if not found index.html
        --render-try-index       Serve index.html when requesting a directory, returns directory listing if not found index.html
        --render-spa             Serve SPA(Single Page Application)
        --assets <path>          Use custom assets to override builtin assets
        --tls-cert <path>        Path to an SSL/TLS certificate to serve with HTTPS
        --tls-key <path>         Path to the SSL/TLS certificate's private key
        --log-format <format>    Customize http log format
        --completions <shell>    Print shell completion script for <shell> [possible values: bash, elvish, fish, powershell, zsh]
    -h, --help                   Print help information
    -V, --version                Print version information
```

## Examples

Serve current working directory

```
dufs
```

Allow all operations like upload/delete/search...

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
dufs -a /@admin:123
```

Listen on a specific port

```
dufs -p 80
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

<details>
<summary><h2>Advanced topics</h2></summary>

### Access Control

Dufs supports path level access control. You can control who can do what on which path with `--auth`/`-a`.

```
dufs -a <path>@<readwrite>
dufs -a <path>@<readwrite>@<readonly>
dufs -a <path>@<readwrite>@*
```

- `<path>`: Protected url path
- `<readwrite>`: Account with readwrite permissions. If dufs is run with `dufs --allow-all`, the permissions are upload/delete/search/view/download. If dufs is run with `dufs --allow-upload`, the permissions are upload/view/download.
- `<readonly>`: Account with readonly permissions. The permissions are search/view/download if dufs allow search, otherwise view/download..

```
dufs -A -a /@admin:admin
```
`admin` has all permissions for all paths.

```
dufs -A -a /@admin:admin@guest:guest
```
`guest` has readonly permissions for all paths.

```
dufs -A -a /@admin:admin@*
```
All paths is public, everyone can view/download it.

```
dufs -A -a /@admin:admin -a /user1@user1:pass1 -a /user2@pass2:user2
```
`user1` has all permissions for `/user1*` path.
`user2` has all permissions for `/user2*` path.

```
dufs -a /@admin:admin
```
Since dufs only allows viewing/downloading, `admin` can only view/download files.

### Hide Paths

Dufs supports hiding paths from directory listings via option `--hidden`.

```
dufs --hidden .git,.DS_Store,tmp
```

`--hidden` also supports a variant glob:

- `?` matches any single character
- `*` matches any (possibly empty) sequence of characters
- `**`, `[..]`, `[!..]` is not supported

```sh
dufs --hidden '.*'
dufs --hidden '*.log,*.lock'
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

### Customize UI

Dufs allows users to customize the UI with their own assets.

```
dufs --assets my-assets
```

You assets folder must contains a entrypoint `index.html`.

`index.html` can use the following planceholder to access internal data.

- `__INDEX_DATA__`: directory listing data
- `__ASSERTS_PREFIX__`: assets url prefix

</details>

## License

Copyright (c) 2022 dufs-developers.

dufs is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.
