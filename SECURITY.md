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

Authorization is evaluated against the requested path and against the canonical target that the path
resolves to. When a path traverses a symbolic link to another path inside the served root, the
effective permission is the stricter of the two: a link never grants access that its target does not
already grant. Targets that cannot be resolved, such as link loops, are denied. Targets outside the
served root have no path rules; they keep being governed by `--allow-symlink` and the requested
path's permission.

This keeps symlink sharing working. A link to a readable target stays readable, and a link to a
write-protected target stays read-only. To grant write access to shared content, grant it on the
target path explicitly, for example:

```
-a 'alice:pass@/home/alice:rw,/shared/team:rw'
```

Placing a link in a `:rw` directory does not grant write access to its target by itself.

When access rules are configured, WebDAV `MOVE` refuses to relocate a symbolic link (or a directory
containing one) to a different directory. A rename within the same directory preserves every
relative target, but it is still authorized as a write against the requested path and, for in-root
targets, against the resolved target. `DELETE` follows the same rule. This prevents an HTTP client
from repointing an existing link while a request is being authorized.

`DELETE` and `MOVE` act on a whole directory, so they are also rejected when the resolved target
subtree contains a path with a stricter rule, even if the request path itself reveals no such rule.
For example, a link inside a `:rw` directory can point at a `:rw` directory that contains a `:ro`
child; removing the linked directory would remove that child. Deleting or renaming the link entry
itself only affects the link and remains allowed.

All entry points follow the same rule, including WebDAV `MOVE`/`COPY` destinations, archive
downloads (`?zip`), search results, and directory listings. Directory listings resolve every entry,
so an entry whose in-root target is not accessible is hidden or shown read-only according to that
target. `--allow-symlink` keeps its meaning with access rules configured: links that resolve outside
the root are served under the requested path's permission, and without the flag they are not served.


## Out of Scope

The following are expected behavior, not vulnerabilities.

### Unauthenticated WebDAV requests list protected directories

An unauthenticated `PROPFIND` lists directory entries, including protected ones. This is required by
WebDAV clients that send no `Authorization` header until challenged with a `401`; hiding the entries
would leave them seeing only the public directory, with no way to reach protected ones. Listing a
name grants no access — reads and writes still return `401`/`403` without credentials.

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
