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
- Basic authentication
- Partial responses (Parallel/Resume download)
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
Duf is a simple file server.

USAGE:
    duf [OPTIONS] [path]

ARGS:
    <path>    Path to a root directory for serving files [default: .]

OPTIONS:
    -a, --auth <user:pass>      Use HTTP authentication
        --no-auth-access        Not required auth when access static files
    -A, --allow-all             Allow all operations
        --allow-delete          Allow delete files/folders
        --allow-symlink         Allow symlink to files/folders outside root directory
        --allow-upload          Allow upload files/folders
    -b, --bind <address>        Specify bind address [default: 0.0.0.0]
        --cors                  Enable CORS, sets `Access-Control-Allow-Origin: *`
    -h, --help                  Print help information
    -p, --port <port>           Specify port to listen on [default: 5000]
        --path-prefix <path>    Specify an url path prefix
        --render-index          Render index.html when requesting a directory
        --render-spa            Render for single-page application
        --tls-cert <path>       Path to an SSL/TLS certificate to serve with HTTPS
        --tls-key <path>        Path to the SSL/TLS certificate's private key
    -V, --version               Print version information
```

## Examples

You can run this command to start serving your current working directory on 127.0.0.1:5000 by default.

```
duf
```

...or specify which folder you want to serve.

```
duf folder_name
```

Allow all operations such as upload, delete

```sh
duf --allow-all
```

Only allow upload operation

```
duf --allow-upload
```

Serve a single page application (SPA)

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