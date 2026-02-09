use std::fmt;

#[derive(Debug)]
pub enum MostroError {
    ParsingInvoiceError,
    ParsingNumberError,
    InvoiceExpiredError,
    MinExpirationTimeError,
    MinAmountError,
}

impl std::error::Error for MostroError {}

impl fmt::Display for MostroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MostroError::ParsingInvoiceError => {
                write!(f, "Failed to parse Lightning invoice: invalid format or checksum")
            }
            MostroError::ParsingNumberError => {
                write!(f, "Failed to parse numeric value: expected valid integer")
            }
            MostroError::InvoiceExpiredError => {
                write!(f, "Lightning invoice has expired and cannot be used")
            }
            MostroError::MinExpirationTimeError => {
                write!(f, "Invoice expiration time is below the minimum required duration")
            }
            MostroError::MinAmountError => {
                write!(f, "Payment amount is below the minimum required value")
            }
        }
    }
}

impl From<lightning_invoice::Bolt11ParseError> for MostroError {
    fn from(_: lightning_invoice::Bolt11ParseError) -> Self {
        MostroError::ParsingInvoiceError
    }
}

impl From<lightning_invoice::ParseOrSemanticError> for MostroError {
    fn from(_: lightning_invoice::ParseOrSemanticError) -> Self {
        MostroError::ParsingInvoiceError
    }
}

impl From<std::num::ParseIntError> for MostroError {
    fn from(_: std::num::ParseIntError) -> Self {
        MostroError::ParsingNumberError
    }
}