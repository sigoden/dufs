# Dufs (Old Name: Duf)

[![CI](https://github.com/sigoden/dufs/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/dufs/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/dufs.svg)](https://crates.io/crates/dufs)

Dufs is a distinctive utility file server that supports static serving, uploading, searching, accessing control, webdav...

![demo](https://user-images.githubusercontent.com/4012553/174486522-7af350e6-0195-4f4a-8480-d9464fc6452f.png)

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
docker run -v `pwd`:/data -p 5000:5000 --rm -it sigoden/dufs /data
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
        --path-prefix <path>     Specify an path prefix
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
    -h, --help                   Print help information
    -V, --version                Print version information
```

## Examples

Serve current working directory

```
dufs
```

Explicitly allow all operations including upload/delete 

```
dufs -A
```

Only allow upload operation

```
dufs --allow-upload
```

Serve a directory

```
dufs Downloads
```

Serve a single file

```
dufs linux-distro.iso
```

Serve index.html when requesting a directory

```
dufs --render-index
```

Serve SPA(Single Page Application)

```
dufs --render-spa
```

Require username/password

```
dufs -a /@admin:123
```

Listen on a specific port

```
dufs -p 80
```

Hide directories from directory listings

```
dufs --hidden .git,.DS_Store
```

Use https

```
dufs --tls-cert my.crt --tls-key my.key
```

## API

Download a file
```
curl http://127.0.0.1:5000/path-to-file
```

Download a folder as zip file

```
curl -o path-to-folder.zip http://127.0.0.1:5000/path-to-folder?zip
```

Upload a file

```
curl --upload-file path-to-file http://127.0.0.1:5000/path-to-file
```

Delete a file/folder

```
curl -X DELETE http://127.0.0.1:5000/path-to-file
```

## Access Control

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
dufs -a /@admin:pass@* -a /ui@designer:pass1 -A
```
- All files/folders are public to view/download.
- Account `admin:pass` can upload/delete/view/download any files/folders.
- Account `designer:pass1` can upload/delete/view/download any files/folders in the `ui` folder.

## License

Copyright (c) 2022 dufs-developers.

dufs is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.