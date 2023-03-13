use anyhow::{Ok, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Orders can be only Buy or Sell
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    ///Buy order option
    Buy,
    ///Sell order option
    Sell,
}

impl FromStr for Kind {
    type Err = ();

    fn from_str(kind: &str) -> std::result::Result<Kind, Self::Err> {
        match kind {
            "Buy" => std::result::Result::Ok(Kind::Buy),
            "Sell" => std::result::Result::Ok(Kind::Sell),
            _ => Err(()),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Each status that an order can have
#[derive(Debug, Deserialize, Serialize, Copy, Clone, ValueEnum, Eq, PartialEq)]
pub enum Status {
    /// Active order
    Active,
    /// Canceled order
    Canceled,
    /// CanceledByAdmin order
    CanceledByAdmin,
    /// CompletedByAdmin order
    CompletedByAdmin,
    /// Dispute order
    Dispute,
    /// Expired order
    Expired,
    /// FiatSent order
    FiatSent,
    /// SettledHoldInvoice order
    SettledHoldInvoice,
    /// Pending order
    Pending,
    /// Success order
    Success,
    /// WaitingBuyerInvoice order
    WaitingBuyerInvoice,
    /// WaitingPayment order
    WaitingPayment,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Action is used to identify each message between Mostro and users
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
pub enum Action {
    Order,
    TakeSell,
    TakeBuy,
    PayInvoice,
    FiatSent,
    Release,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Use this Message to establish communication between users and Mostro
#[derive(Debug, Deserialize, Serialize)]
pub struct Message {
    pub version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<Uuid>,
    pub action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
}

/// Message content
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum Content {
    Order(Order),
    PaymentRequest(String),
    PayHoldInvoice(Order, String),
}

#[allow(dead_code)]
impl Message {
    pub fn new(
        version: u8,
        order_id: Option<Uuid>,
        action: Action,
        content: Option<Content>,
    ) -> Self {
        Self {
            version,
            order_id,
            action,
            content,
        }
    }

    /// New message from json string
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
    /// Get message as json string
    pub fn as_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self)?)
    }

    /// Verify if is valid message
    pub fn verify(&self) -> bool {
        match &self.action {
            Action::Order => matches!(&self.content, Some(Content::Order(_))),
            Action::TakeSell => {
                if self.order_id.is_none() {
                    return false;
                }
                matches!(&self.content, Some(Content::PaymentRequest(_)))
            }
            Action::TakeBuy | Action::FiatSent | Action::Release => {
                if self.order_id.is_none() {
                    return false;
                }
                true
            }
            Action::PayInvoice => {
                todo!()
            }
        }
    }

    pub fn get_order(&self) -> Option<&Order> {
        if self.action != Action::Order {
            return None;
        }
        match &self.content {
            Some(Content::Order(o)) => Some(o),
            _ => None,
        }
    }

    pub fn get_payment_request(&self) -> Option<String> {
        if self.action != Action::TakeSell {
            return None;
        }
        match &self.content {
            Some(Content::PaymentRequest(pr)) => Some(pr.to_owned()),
            _ => None,
        }
    }
}

/// Mostro Order
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Order {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    pub kind: Kind,
    pub status: Status,
    pub amount: Option<u32>,
    pub fiat_code: String,
    pub fiat_amount: u32,
    pub payment_method: String,
    pub prime: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer_invoice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
}

#[allow(dead_code)]
impl Order {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Option<Uuid>,
        kind: Kind,
        status: Status,
        amount: Option<u32>,
        fiat_code: String,
        fiat_amount: u32,
        payment_method: String,
        prime: i8,
        buyer_invoice: Option<String>,
        created_at: Option<u64>,
    ) -> Self {
        Self {
            id,
            kind,
            status,
            amount,
            fiat_code,
            fiat_amount,
            payment_method,
            prime,
            buyer_invoice,
            created_at,
        }
    }
    /// New order from json string
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    /// Get order as json string
    pub fn as_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self)?)
    }
}
