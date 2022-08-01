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
    dufs [OPTIONS] [--] [path]

ARGS:
    <path>    Specific path to serve [default: .]

OPTIONS:
    -b, --bind <addr>...         Specify bind address
    -p, --port <port>            Specify port to listen on [default: 5000]
        --path-prefix <path>     Specify a path prefix
        --hidden <value>         Hide directories from directory listings, separated by `,`
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
<summary><h2>Advance topics</h2></summary>

### Access Control

Dufs supports path level access control. You can control who can do what on which path with `--auth`/`-a`.

```
dufs -a <path>@<readwrite>[@<readonly>|@*]
```

- `<path>`: Protected url path
- `<readwrite>`: Account with upload/delete/view/download permission, required
- `<readonly>`: Account with view/download permission, optional

> `*` means `<path>` is public, everyone can view/download it.

For example:

```
dufs -a /@admin:pass1@* -a /ui@designer:pass2 -A
```
- All files/folders are public to view/download.
- Account `admin:pass1` can upload/delete/view/download any files/folders.
- Account `designer:pass2` can upload/delete/view/download any files/folders in the `ui` folder.


### Hide

Dufs supports hiding directories/files via option `--hidden`.

```
dufs --hidden .git,.DS_Store
```

`--hidden` supports a variant glob:

- `?` matches any single character
- `*` matches any (possibly empty) sequence of characters
- no `**`, `[..]`, `[!..]`

Hide all hidden directories/files

```
dufs --hidden '.*'
```

### Log Format

Dufs supports customize http log format via option `--log-format`.

The default format is `'$remote_addr "$request" $status'`.

| variable     | description                                                               |
| ------------ | ------------------------------------------------------------------------- |
| $remote_addr | client address                                                            |
| $remote_user | user name supplied with authentication                                    |
| $request     | full original request line                                                |
| $status      | response status                                                           |
| $http_       | arbitrary request header field. examples: $http_user_agent, $http_referer |

> use `dufs --log-format=''` to disable http log

</details>

## License

Copyright (c) 2022 dufs-developers.

dufs is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.