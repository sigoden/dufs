# Security Policy

## Supported Versions

The latest release of *dufs* is supported. The fixes for any security issues found will be included
in the next release.


## Reporting a Vulnerability

Please [use *dufs*'s security advisory reporting tool provided by
GitHub](https://github.com/sigoden/dufs/security/advisories/new) to report security issues.

We strive to fix security issues as quickly as possible. Across the industry, often the developers'
slowness in developing and releasing a fix is the biggest delay in the process; we take pride in
minimizing this delay as much as we practically can. We encourage you to also minimize the delay
between when you find an issue and when you contact us. You do not need to convince us to take your
report seriously. You don't need to create a PoC or a patch if that would slow down your reporting.
You don't need an elaborate write-up. A short, informal note about the issue is good. We can always
communicate later to fill in any details we need after that first note is shared with us.


## Access Control and Symbolic Links

Authorization checks both the requested path and its canonical target, and the stricter result wins:
a symlink never grants access its target lacks. Unresolvable targets, such as link loops, are denied.
Out-of-root targets have no path rules and are governed by `--allow-symlink` (not served without it)
plus the requested path's permission.

To grant write access to shared content, grant it on the target path explicitly; a link inside a
`:rw` directory does not inherit write access to its target:

```
-a 'alice:pass@/home/alice:rw,/shared/team:rw'
```

With access rules configured, `MOVE` refuses to relocate a symlink (or a directory containing one)
to another directory, and both `MOVE` and `DELETE` are rejected when the resolved subtree contains a
stricter rule. Removing or renaming the link entry itself remains allowed. A same-directory rename
is authorized as a write against the requested path and, for in-root targets, the resolved target.
All entry points follow these rules, including `MOVE`/`COPY` destinations, `?zip` archives, search
results, and listings, which resolve entries and hide or downgrade inaccessible ones.


## Cross-Origin Resource Sharing (CORS)

`--enable-cors` is an explicit opt-in that lets every origin call the API. Responses carry
`Access-Control-Allow-Origin: *`, wildcard methods and exposed headers, and allow `Authorization`, so
any web page can list, download, upload, or delete content whenever the corresponding global
permissions are enabled. *Dufs* uses HTTP Basic/Digest rather than cookies, so a page can only act
with credentials it already knows. There is no origin allowlist: the flag always applies to every
origin.

Enabling it exposes the API to any website. Use `--auth` for sensitive content, and avoid the flag
on an unauthenticated writable instance that is only meant to be reached locally.


## Out of Scope

The following are expected behavior, not vulnerabilities.

### Unauthenticated WebDAV requests list protected directories

An unauthenticated `PROPFIND` lists directory entries, including protected ones. This is required by
WebDAV clients that send no `Authorization` header until challenged with a `401`; hiding the entries
would leave them seeing only the public directory, with no way to reach protected ones. Listing a
name grants no access — reads and writes still return `401`/`403` without credentials.

### CORS delegates access to every origin

`--enable-cors` is documented as allowing every origin to call the API. A web page can therefore
drive a CORS-enabled instance in the same way as any other client that *dufs* allows. This is the
purpose of the flag, not a bypass of the server's permissions. See [Cross-Origin Resource Sharing
(CORS)](#cross-origin-resource-sharing-cors).

### XSS from files served by dufs

Files are served inline with their natural content type, including HTML/SVG/XML, so a malicious file
executes script on *dufs*'s origin. This is not handled because *dufs* exists to serve the files it
stores: forcing this active content to download broke opening HTML files from the web UI and was
reverted. Operators are responsible for not hosting untrusted content and for isolating *dufs* on
its own origin.

### Path authorization and hard links

A hard link is a second directory entry for an existing file. Path canonicalization does not merge
hard links, so *dufs* cannot tell that two paths refer to the same object. Creating a hard link
requires local filesystem access, which *dufs* does not provide over HTTP. Treat local write access
to a served tree as equivalent to the permissions *dufs* grants through that path, not as a trust
boundary between accounts.
