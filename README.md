# Duf

[![CI](https://github.com/sigoden/duf/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/duf/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/duf.svg)](https://crates.io/crates/duf)

Duf is a simple file server. Support static serve, search, upload, webdav...

![demo](https://user-images.githubusercontent.com/4012553/171526189-09afc2de-793f-4216-b3d5-31ea408d3610.png)

## Features

- Serve static files
- Download folder as zip file
- Upload files and folders (Drag & Drop)
- Search files
- Partial responses (Parallel/Resume download)
- Authentication
- Support https
- Support webdav
- Easy to use with curl

## Install

### With cargo

```
cargo install duf
```

### With docker

```
docker run -v /tmp:/tmp -p 5000:5000 --rm -it docker.io/sigoden/duf /tmp
```

### Binaries on macOS, Linux, Windows

Download from [Github Releases](https://github.com/sigoden/duf/releases), unzip and add duf to your $PATH.

## CLI

```
Duf is a simple file server. - https://github.com/sigoden/duf

USAGE:
    duf [OPTIONS] [--] [path]

ARGS:
    <path>    Path to a root directory for serving files [default: .]

OPTIONS:
    -b, --bind <addr>...        Specify bind address
    -p, --port <port>           Specify port to listen on [default: 5000]
        --path-prefix <path>    Specify an url path prefix
    -a, --auth <user:pass>      Use HTTP authentication
        --no-auth-access        Not required auth when access static files
    -A, --allow-all             Allow all operations
        --allow-upload          Allow upload files/folders
        --allow-delete          Allow delete files/folders
        --allow-symlink         Allow symlink to files/folders outside root directory
        --render-index          Render index.html when requesting a directory
        --render-try-index      Try rendering index.html when requesting a directory
        --render-spa            Render for single-page application
        --cors                  Enable CORS, sets `Access-Control-Allow-Origin: *`
        --tls-cert <path>       Path to an SSL/TLS certificate to serve with HTTPS
        --tls-key <path>        Path to the SSL/TLS certificate's private key
    -h, --help                  Print help information
    -V, --version               Print version information
```

## Examples

Serve current working directory, no upload/delete

```
duf
```

Allow upload/delete

```
duf -A
```

Listen on a specific port

```
duf -p 80
```

Protect with authentication

```
duf -a admin:admin
```

For a single page application (SPA)

```
duf --render-spa
```

Use https

```
duf --tls-cert my.crt --tls-key my.key
```

## API

Download a file
```
curl http://127.0.0.1:5000/some-file
```

Download a folder as zip file

```
curl -o some-folder.zip http://127.0.0.1:5000/some-folder?zip
```

Upload a file

```
curl --upload-file some-file http://127.0.0.1:5000/some-file
```

Delete a file/folder

```
curl -X DELETE http://127.0.0.1:5000/some-file
```

## License

Copyright (c) 2022 duf-developers.

duf is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.