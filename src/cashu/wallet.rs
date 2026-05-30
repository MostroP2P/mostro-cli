use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use cdk::nuts::{
    Conditions, CurrencyUnit, P2PKWitness, Proof, PublicKey, SecretKey, SigFlag,
    SpendingConditions, Token, Witness,
};
use cdk::wallet::{ReceiveOptions, SendOptions, Wallet};
use cdk::Amount;
use cdk_redb::WalletRedbDatabase;

pub struct CashuWallet {
    inner: Wallet,
}

impl CashuWallet {
    /// Create a wallet whose proof state is persisted at `db_path`.
    /// In Phase 3 this path will come from the application config.
    pub fn new(mint_url: &str, seed: [u8; 64], db_path: &Path) -> Result<Self> {
        let localstore = WalletRedbDatabase::new(db_path)?;
        let wallet = Wallet::new(mint_url, CurrencyUnit::Sat, Arc::new(localstore), seed, None)?;
        Ok(Self { inner: wallet })
    }

    /// Swap unencumbered proofs into a 2-of-3 P2PK locked token.
    ///
    /// The resulting token requires any 2 signatures from the set
    /// {buyer_pubkey, seller_pubkey, mostro_pubkey} to be spent.
    /// Returns the serialised token string ready to be shared over Nostr.
    pub async fn swap_to_p2pk_locked(
        &self,
        amount_sat: u64,
        buyer_pubkey: PublicKey,
        seller_pubkey: PublicKey,
        mostro_pubkey: PublicKey,
    ) -> Result<String> {
        let conditions = Conditions::new(
            None,
            Some(vec![seller_pubkey, mostro_pubkey]),
            None,
            Some(2),
            Some(SigFlag::SigInputs),
            None,
        )?;
        let spending_conditions = SpendingConditions::new_p2pk(buyer_pubkey, Some(conditions));
        let prepared = self
            .inner
            .prepare_send(
                Amount::from(amount_sat),
                SendOptions {
                    conditions: Some(spending_conditions),
                    include_fee: true,
                    ..Default::default()
                },
            )
            .await?;
        let token = prepared.confirm(None).await?;
        Ok(token.to_string())
    }

    /// Add NUT-11 P2PK witness signatures to a set of proofs in-place.
    pub fn sign_proofs(proofs: &mut [Proof], secret_key: SecretKey) -> Result<()> {
        for proof in proofs.iter_mut() {
            proof.sign_p2pk(secret_key.clone())?;
        }
        Ok(())
    }

    /// Redeem a P2PK-locked token by providing the required signing keys.
    ///
    /// The caller must supply the secret keys whose public keys are listed in
    /// the token's spending conditions. CDK will add the witness signatures and
    /// submit the swap to the mint.
    pub async fn redeem_with_keys(
        &self,
        token_str: &str,
        signing_keys: Vec<SecretKey>,
    ) -> Result<u64> {
        let amount = self
            .inner
            .receive(
                token_str,
                ReceiveOptions {
                    p2pk_signing_keys: signing_keys,
                    ..Default::default()
                },
            )
            .await?;
        Ok(u64::from(amount))
    }

    /// Verify that a proof carries the expected 2-of-3 P2PK spending condition
    /// for the Mostro escrow participants.
    pub fn verify_2of3_condition(
        proof: &Proof,
        buyer_pk: &PublicKey,
        seller_pk: &PublicKey,
        mostro_pk: &PublicKey,
    ) -> Result<()> {
        let spending_condition: Option<SpendingConditions> = (&proof.secret).try_into().ok();
        let spending_condition = spending_condition
            .ok_or_else(|| anyhow::anyhow!("Proof has no spending condition"))?;

        let all_pubkeys = spending_condition
            .pubkeys()
            .ok_or_else(|| anyhow::anyhow!("Proof has no P2PK pubkeys"))?;

        if !all_pubkeys.contains(buyer_pk)
            || !all_pubkeys.contains(seller_pk)
            || !all_pubkeys.contains(mostro_pk)
        {
            return Err(anyhow::anyhow!(
                "Proof missing expected pubkeys in 2-of-3 condition"
            ));
        }

        let num_sigs = spending_condition
            .num_sigs()
            .ok_or_else(|| anyhow::anyhow!("Proof has no num_sigs requirement"))?;

        if num_sigs != 2 {
            return Err(anyhow::anyhow!(
                "Expected 2-of-3 threshold but found {}-of-N",
                num_sigs
            ));
        }

        Ok(())
    }

    /// Sign every proof in a serialized Cashu token with the given secret key.
    ///
    /// Each proof's NUT-11 P2PK witness receives one additional Schnorr signature.
    /// Existing witness signatures (e.g. from a co-signer) are preserved.
    /// Returns the re-serialized token string.
    pub fn sign_token(token_str: &str, secret_key: SecretKey) -> Result<String> {
        let mut token: Token = token_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse token: {:?}", e))?;

        match &mut token {
            Token::TokenV3(v3) => {
                for t in &mut v3.token {
                    for proof in &mut t.proofs {
                        sign_proof_witness(&mut proof.witness, &proof.secret, &secret_key)?;
                    }
                }
            }
            Token::TokenV4(v4) => {
                for t in &mut v4.token {
                    for proof in &mut t.proofs {
                        sign_proof_witness(&mut proof.witness, &proof.secret, &secret_key)?;
                    }
                }
            }
        }

        Ok(token.to_string())
    }

    pub async fn total_balance(&self) -> Result<u64> {
        let bal = self.inner.total_balance().await?;
        Ok(u64::from(bal))
    }
}

fn sign_proof_witness(
    witness: &mut Option<Witness>,
    secret: &cdk::secret::Secret,
    sk: &SecretKey,
) -> Result<()> {
    let sig = sk
        .sign(&secret.to_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to sign proof: {:?}", e))?;
    let sig_str = sig.to_string();
    match witness.as_mut() {
        Some(w) => w.add_signatures(vec![sig_str]),
        None => {
            let mut p2pk_w = Witness::P2PKWitness(P2PKWitness::default());
            p2pk_w.add_signatures(vec![sig_str]);
            *witness = Some(p2pk_w);
        }
    }
    Ok(())
}
