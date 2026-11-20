use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("ZIP: {0}")]
    Zip(String),
    #[error("7z: {0}")]
    SevenZ(String),
    #[error("RAR: {0}")]
    Rar(String),
    #[error("O arquivo está protegido por senha")]
    PasswordRequired,
    #[error("Senha incorreta ou arquivo corrompido")]
    WrongPassword,
    #[error("Operação cancelada")]
    Cancelled,
    #[error("Não suportado: {0}")]
    Unsupported(String),
    #[error("Dados corrompidos: {0}")]
    Corrupt(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Código estável para a interface decidir o que fazer (pedir senha, ignorar cancelamento...).
    pub fn code(&self) -> &'static str {
        match self {
            Error::PasswordRequired => "password_required",
            Error::WrongPassword => "wrong_password",
            Error::Cancelled => "cancelled",
            Error::Unsupported(_) => "unsupported",
            Error::Corrupt(_) => "corrupt",
            _ => "error",
        }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        use zip::result::ZipError;
        match e {
            ZipError::InvalidPassword => Error::WrongPassword,
            ZipError::UnsupportedArchive(msg) if msg.contains("assword") => Error::PasswordRequired,
            ZipError::Io(io) => io_or_cancel(io),
            other => Error::Zip(other.to_string()),
        }
    }
}

impl From<sevenz_rust2::Error> for Error {
    fn from(e: sevenz_rust2::Error) -> Self {
        use sevenz_rust2::Error as E;
        match e {
            E::PasswordRequired => Error::PasswordRequired,
            E::MaybeBadPassword(_) => Error::WrongPassword,
            E::Io(io, msg) => {
                if is_cancel(&io) {
                    Error::Cancelled
                } else if msg.is_empty() {
                    Error::Io(io)
                } else {
                    Error::SevenZ(format!("{msg}: {io}"))
                }
            }
            E::ChecksumVerificationFailed => Error::Corrupt("CRC não confere".into()),
            other => Error::SevenZ(other.to_string()),
        }
    }
}

impl From<unrar::error::UnrarError> for Error {
    fn from(e: unrar::error::UnrarError) -> Self {
        use unrar::error::Code;
        match e.code {
            Code::MissingPassword => Error::PasswordRequired,
            Code::BadPassword => Error::WrongPassword,
            Code::BadData => Error::Corrupt("CRC não confere".into()),
            _ => Error::Rar(e.to_string()),
        }
    }
}

/// Marca usada nos `io::Error` que saem dos leitores com progresso quando o usuário cancela
/// (`Other`, não `Interrupted`: `io::copy` repete leituras interrompidas).
pub(crate) const CANCEL_MARK: &str = "rustzip:cancelled";

pub(crate) fn cancel_io() -> io::Error {
    io::Error::new(io::ErrorKind::Other, CANCEL_MARK)
}

pub(crate) fn is_cancel(e: &io::Error) -> bool {
    e.to_string() == CANCEL_MARK
}

pub(crate) fn io_or_cancel(e: io::Error) -> Error {
    if is_cancel(&e) { Error::Cancelled } else { Error::Io(e) }
}
