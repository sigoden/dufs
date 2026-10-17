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
