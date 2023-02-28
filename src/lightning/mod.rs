use crate::error::MostroError;
use chrono::prelude::*;
use chrono::Duration;
use dotenvy::var;
use lightning_invoice::{Invoice, SignedRawInvoice};
use std::str::FromStr;

/// Verify if an invoice is valid
pub fn is_valid_invoice(payment_request: &str) -> Result<Invoice, MostroError> {
    let invoice = Invoice::from_str(payment_request)?;
    if invoice.is_expired() {
        return Err(MostroError::InvoiceExpiredError);
    }

    Ok(invoice)
}
