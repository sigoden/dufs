use crate::http_utils::body_full;
use crate::server::Response;

use hyper::header::HeaderValue;
use hyper::StatusCode;

/// Response header carrying the machine-readable error reason.
pub const ERROR_CODE_HEADER: &str = "x-dufs-error-code";

/// Machine-readable reason for an error response.
///
/// Several different situations share the same HTTP status code (for example
/// an account without write permission and a `--allow-upload` disabled
/// server both return 403). Clients can tell them apart through
/// [`ERROR_CODE_HEADER`].
///
/// Error codes are intended for the dufs web UI and HTTP API clients. They are
/// **not applicable to WebDAV responses**, which report errors through DAV XML
/// error bodies as defined by RFC 4918.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DufsErrorCode {
    // 400
    InvalidPath,
    InvalidUploadOffset,
    InvalidDepth,
    InvalidDestination,
    // 403
    Forbidden,
    ReadonlyDescendant,
    UploadDisabled,
    DeleteDisabled,
    HashDisabled,
    IsADirectory,
    InternalError,
    // 404
    ArchiveDisabled,
    // 405
    MissingUploadOffset,
    AlreadyExists,
    MethodNotAllowed,
    // 412
    PreconditionFailed,
    DestinationExists,
}

impl DufsErrorCode {
    /// Value of the [`ERROR_CODE_HEADER`] response header.
    pub const fn as_str(self) -> &'static str {
        use DufsErrorCode::*;
        match self {
            InvalidPath => "invalid-path",
            InvalidUploadOffset => "invalid-upload-offset",
            InvalidDepth => "invalid-depth",
            InvalidDestination => "invalid-destination",
            Forbidden => "forbidden",
            ReadonlyDescendant => "readonly-descendant",
            UploadDisabled => "upload-disabled",
            DeleteDisabled => "delete-disabled",
            HashDisabled => "hash-disabled",
            IsADirectory => "is-a-directory",
            InternalError => "internal-error",
            ArchiveDisabled => "archive-disabled",
            MissingUploadOffset => "missing-upload-offset",
            AlreadyExists => "already-exists",
            MethodNotAllowed => "method-not-allowed",
            PreconditionFailed => "precondition-failed",
            DestinationExists => "destination-exists",
        }
    }

    pub const fn status(self) -> StatusCode {
        use DufsErrorCode::*;
        match self {
            InvalidPath | InvalidUploadOffset | InvalidDepth | InvalidDestination => {
                StatusCode::BAD_REQUEST
            }
            Forbidden | ReadonlyDescendant | UploadDisabled | DeleteDisabled | HashDisabled
            | IsADirectory | InternalError => StatusCode::FORBIDDEN,
            ArchiveDisabled => StatusCode::NOT_FOUND,
            MissingUploadOffset | AlreadyExists | MethodNotAllowed => {
                StatusCode::METHOD_NOT_ALLOWED
            }
            PreconditionFailed | DestinationExists => StatusCode::PRECONDITION_FAILED,
        }
    }

    /// Default human-readable body. An empty string means no body.
    pub const fn message(self) -> &'static str {
        use DufsErrorCode::*;
        match self {
            InvalidPath => "Invalid Path",
            InvalidUploadOffset => "Invalid X-Update-Range Header",
            InvalidDepth => "Invalid depth: only 0 and 1 are allowed.",
            InvalidDestination => "Invalid Destination",
            Forbidden => "Forbidden",
            ReadonlyDescendant => "Forbidden: the target directory contains read-only entries",
            UploadDisabled => "Forbidden: upload is disabled",
            DeleteDisabled => "Forbidden: delete is disabled",
            HashDisabled => "Forbidden: hash is disabled",
            IsADirectory => "Forbidden: target is a directory",
            InternalError => "Forbidden: failed to access the directory",
            ArchiveDisabled => "Not Found",
            MissingUploadOffset => "",
            AlreadyExists => "Already exists",
            MethodNotAllowed | PreconditionFailed | DestinationExists => "",
        }
    }

    /// Write this error, including its code header and default body, into
    /// `res`.
    pub fn respond(self, res: &mut Response) {
        self.respond_with(res, self.message());
    }

    /// Same as [`Self::respond`], but with a dynamic human-readable body.
    pub fn respond_with(self, res: &mut Response, message: &str) {
        *res.status_mut() = self.status();
        res.headers_mut()
            .insert(ERROR_CODE_HEADER, HeaderValue::from_static(self.as_str()));
        if !message.is_empty() {
            *res.body_mut() = body_full(message.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique() {
        use DufsErrorCode::*;
        const ALL: [DufsErrorCode; 17] = [
            InvalidPath,
            InvalidUploadOffset,
            InvalidDepth,
            InvalidDestination,
            Forbidden,
            ReadonlyDescendant,
            UploadDisabled,
            DeleteDisabled,
            HashDisabled,
            IsADirectory,
            InternalError,
            ArchiveDisabled,
            MissingUploadOffset,
            AlreadyExists,
            MethodNotAllowed,
            PreconditionFailed,
            DestinationExists,
        ];
        let mut codes = std::collections::HashSet::new();
        for code in ALL {
            assert!(
                codes.insert(code.as_str()),
                "duplicate code {}",
                code.as_str()
            );
        }
    }
}
