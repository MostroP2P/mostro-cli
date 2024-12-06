use crate::util::get_mcli_path;
use mostro_core::NOSTR_REPLACEABLE_EVENT_KIND;
use nip06::FromMnemonic;
use nostr_sdk::prelude::*;
use sqlx::pool::Pool;
use sqlx::Sqlite;
use sqlx::SqlitePool;
use std::fs::File;
use std::path::Path;

pub async fn connect() -> Result<Pool<Sqlite>, sqlx::Error> {
    let mcli_dir = get_mcli_path();
    let mcli_db_path = format!("{}/mcli.db", mcli_dir);
    let db_url = format!("sqlite://{}", mcli_db_path);
    let pool: Pool<Sqlite>;
    if !Path::exists(Path::new(&mcli_db_path)) {
        if let Err(res) = File::create(&mcli_db_path) {
            println!("Error in creating db file: {}", res);
            return Err(sqlx::Error::Io(res));
        }
        pool = SqlitePool::connect(&db_url).await?;
        println!("Creating database file with orders table...");
        sqlx::query(
            r#"
          CREATE TABLE IF NOT EXISTS orders (
              id TEXT PRIMARY KEY,
              kind TEXT,
              status TEXT,
              amount INTEGER NOT NULL,
              fiat_code TEXT NOT NULL,
              min_amount INTEGER,
              max_amount INTEGER,
              fiat_amount INTEGER NOT NULL,
              payment_method TEXT NOT NULL,
              premium INTEGER NOT NULL,
              master_buyer_pubkey TEXT,
              master_seller_pubkey TEXT,
              buyer_invoice TEXT,
              created_at INTEGER,
              expires_at INTEGER,
              buyer_token INTEGER,
              seller_token INTEGER
          );
          CREATE TABLE IF NOT EXISTS users (
              i0_pubkey TEXT PRIMARY KEY,
              mnemonic TEXT,
              last_trade_index INTEGER,
              created_at INTEGER
          );
          "#,
        )
        .execute(&pool)
        .await?;

        let mnemonic = match Mnemonic::generate(12) {
            Ok(m) => m.to_string(),
            Err(e) => {
                println!("Error generating mnemonic: {}", e);
                return Err(sqlx::Error::Decode(Box::new(e)));
            }
        };
        let user = User::new(mnemonic, &pool).await.unwrap();
        println!("User created with pubkey: {}", user.i0_pubkey);
    } else {
        pool = SqlitePool::connect(&db_url).await?;
    }

    Ok(pool)
}

#[derive(Debug, Default, Clone, sqlx::FromRow)]
pub struct User {
    /// The user's ID is the identity pubkey
    pub i0_pubkey: String,
    pub mnemonic: String,
    pub last_trade_index: Option<i64>,
    pub created_at: i64,
}

impl User {
    pub async fn new(
        mnemonic: String,
        pool: &SqlitePool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut user = User::default();
        let account = NOSTR_REPLACEABLE_EVENT_KIND as u32;
        let i0_keys =
            Keys::from_mnemonic_advanced(&mnemonic, None, Some(account), Some(0), Some(0))?;
        user.i0_pubkey = i0_keys.public_key().to_string();
        user.created_at = chrono::Utc::now().timestamp();
        user.mnemonic = mnemonic;
        sqlx::query(
            r#"
                  INSERT INTO users (i0_pubkey, mnemonic, created_at)
                  VALUES (?, ?, ?)
                "#,
        )
        .bind(&user.i0_pubkey)
        .bind(&user.mnemonic)
        .bind(user.created_at)
        .execute(pool)
        .await?;

        Ok(user)
    }
    // Chainable setters
    pub fn set_mnemonic(&mut self, mnemonic: String) -> &mut Self {
        self.mnemonic = mnemonic;
        self
    }

    pub fn set_last_trade_index(&mut self, last_trade_index: i64) -> &mut Self {
        self.last_trade_index = Some(last_trade_index);
        self
    }

    // Applying changes to the database
    pub async fn save(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
        sqlx::query(
            r#"
              UPDATE users 
              SET mnemonic = ?
              WHERE i0_pubkey = ?
              "#,
        )
        .bind(&self.mnemonic)
        .bind(&self.i0_pubkey)
        .execute(pool)
        .await?;

        println!(
            "User with i0 pubkey {} updated in the database.",
            self.i0_pubkey
        );

        Ok(())
    }

    pub async fn get(pool: SqlitePool) -> Result<User, Box<dyn std::error::Error>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT i0_pubkey, mnemonic, last_trade_index, created_at
            FROM users
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await?;

        Ok(user)
    }

    pub async fn get_next_trade_index(pool: SqlitePool) -> Result<u32, Box<dyn std::error::Error>> {
        let user = User::get(pool).await?;
        match user.last_trade_index {
            Some(index) => Ok((index + 1) as u32),
            None => Ok(1),
        }
    }

    pub async fn get_identity_keys(pool: &SqlitePool) -> Result<Keys, Box<dyn std::error::Error>> {
        let user = User::get(pool.clone()).await?;
        let account = NOSTR_REPLACEABLE_EVENT_KIND as u32;
        let keys =
            Keys::from_mnemonic_advanced(&user.mnemonic, None, Some(account), Some(0), Some(0))?;

        Ok(keys)
    }

    pub async fn get_next_trade_keys(
        pool: &SqlitePool,
    ) -> Result<(Keys, u32), Box<dyn std::error::Error>> {
        let trade_index = User::get_next_trade_index(pool.clone()).await?;
        let user = User::get(pool.clone()).await?;
        let account = NOSTR_REPLACEABLE_EVENT_KIND as u32;
        let keys = Keys::from_mnemonic_advanced(
            &user.mnemonic,
            None,
            Some(account),
            Some(0),
            Some(trade_index),
        )?;

        Ok((keys, trade_index))
    }
}

#[derive(Debug, Default, Clone)]
pub struct Order {
    pub id: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub amount: i64,
    pub fiat_code: String,
    pub min_amount: Option<i64>,
    pub max_amount: Option<i64>,
    pub fiat_amount: i64,
    pub payment_method: String,
    pub premium: i64,
    pub buyer_trade_pubkey: Option<String>,
    pub seller_trade_pubkey: Option<String>,
    pub buyer_invoice: Option<String>,
    pub created_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub buyer_token: Option<u16>,
    pub seller_token: Option<u16>,
}

impl Order {
    // Setters encadenables
    pub fn set_kind(&mut self, kind: String) -> &mut Self {
        self.kind = Some(kind);
        self
    }

    pub fn set_status(&mut self, status: String) -> &mut Self {
        self.status = Some(status);
        self
    }

    pub fn set_amount(&mut self, amount: i64) -> &mut Self {
        self.amount = amount;
        self
    }

    pub fn set_fiat_code(&mut self, fiat_code: String) -> &mut Self {
        self.fiat_code = fiat_code;
        self
    }

    pub fn set_min_amount(&mut self, min_amount: i64) -> &mut Self {
        self.min_amount = Some(min_amount);
        self
    }

    pub fn set_max_amount(&mut self, max_amount: i64) -> &mut Self {
        self.max_amount = Some(max_amount);
        self
    }

    pub fn set_fiat_amount(&mut self, fiat_amount: i64) -> &mut Self {
        self.fiat_amount = fiat_amount;
        self
    }

    pub fn set_payment_method(&mut self, payment_method: String) -> &mut Self {
        self.payment_method = payment_method;
        self
    }

    pub fn set_premium(&mut self, premium: i64) -> &mut Self {
        self.premium = premium;
        self
    }

    pub fn set_buyer_trade_key(&mut self, buyer_trade_pubkey: String) -> &mut Self {
        self.buyer_trade_pubkey = Some(buyer_trade_pubkey);
        self
    }

    pub fn set_seller_trade_key(&mut self, seller_trade_pubkey: String) -> &mut Self {
        self.seller_trade_pubkey = Some(seller_trade_pubkey);
        self
    }

    // Applying changes to the database
    pub async fn save(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
        // Validation if an identity document is present
        if let Some(ref id) = self.id {
            sqlx::query(
                r#"
              UPDATE orders 
              SET kind = ?, status = ?, amount = ?, fiat_code = ?, min_amount = ?, max_amount = ?, 
                  fiat_amount = ?, payment_method = ?, premium = ?, buyer_trade_pubkey = ?, 
                  seller_trade_pubkey = ?, buyer_invoice = ?, created_at = ?, expires_at = ?, 
                  buyer_token = ?, seller_token = ?
              WHERE id = ?
              "#,
            )
            .bind(&self.kind)
            .bind(&self.status)
            .bind(self.amount)
            .bind(&self.fiat_code)
            .bind(self.min_amount)
            .bind(self.max_amount)
            .bind(self.fiat_amount)
            .bind(&self.payment_method)
            .bind(self.premium)
            .bind(&self.buyer_trade_pubkey)
            .bind(&self.seller_trade_pubkey)
            .bind(&self.buyer_invoice)
            .bind(self.created_at)
            .bind(self.expires_at)
            .bind(self.buyer_token)
            .bind(self.seller_token)
            .bind(id)
            .execute(pool)
            .await?;

            println!("Order with id {} updated in the database.", id);
        } else {
            return Err("Order must have an ID to be updated.".into());
        }

        Ok(())
    }
}
