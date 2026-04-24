#![no_std]
#![allow(deprecated)] // env.events().publish() is deprecated in SDK 25 but .emit()
                      // is only available in SDK 26+. Suppress until we upgrade.

use soroban_sdk::{
    contract, contractimpl, contracterror,
    symbol_short, Address, Env, Map, Symbol, Vec,
};

// ─── Error codes ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum WalletError {
    NotOwner            = 1,
    AlreadyInitialized  = 2,
    NotATrustedContact  = 3,
    AlreadyVoted        = 4,
    NoRecoveryPending   = 5,
    RecoveryAlreadyOpen = 6,
    TooFewContacts      = 7,
    InvalidThreshold    = 8,
    ContactAlreadyAdded = 9,
    MaxContactsReached  = 10,
    SelfRecovery        = 11,
}

// ─── Storage keys ─────────────────────────────────────────────────────────────

const OWNER:     Symbol = symbol_short!("OWNER");
const THRESHOLD: Symbol = symbol_short!("THRESH");
const CONTACTS:  Symbol = symbol_short!("CONTACTS");
const RECOVERY:  Symbol = symbol_short!("RECOVERY");
const VOTES:     Symbol = symbol_short!("VOTES");

// ─── Event topic symbols (all ≤ 9 chars) ─────────────────────────────────────

const EV_INIT:    Symbol = symbol_short!("init");
const EV_ADD:     Symbol = symbol_short!("addCont");   // was "addContact" (10) → fixed
const EV_REMOVE:  Symbol = symbol_short!("rmCont");
const EV_THRESH:  Symbol = symbol_short!("setThresh");
const EV_VOTE:    Symbol = symbol_short!("vote");
const EV_CANCEL:  Symbol = symbol_short!("cancelRec");
const EV_RECOVER: Symbol = symbol_short!("recovered");

const MAX_CONTACTS: u32 = 10;

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct SocialRecoveryWallet;

#[contractimpl]
impl SocialRecoveryWallet {

    /// Initialise the wallet. Can only be called once.
    pub fn initialize(env: Env, owner: Address, threshold: u32) -> Result<(), WalletError> {
        if env.storage().instance().has(&OWNER) {
            return Err(WalletError::AlreadyInitialized);
        }
        if threshold == 0 {
            return Err(WalletError::InvalidThreshold);
        }

        owner.require_auth();

        env.storage().instance().set(&OWNER, &owner);
        env.storage().instance().set(&THRESHOLD, &threshold);
        env.storage().instance().set(&CONTACTS, &Vec::<Address>::new(&env));

        env.events().publish((EV_INIT, owner), threshold);
        Ok(())
    }

    // ── Trusted-contact management ────────────────────────────────────────────

    /// Add a trusted contact. Only the owner may call this.
    pub fn add_contact(env: Env, contact: Address) -> Result<(), WalletError> {
        let owner = Self::require_owner(&env)?;

        if contact == owner {
            return Err(WalletError::SelfRecovery);
        }

        let mut contacts: Vec<Address> = env.storage().instance().get(&CONTACTS).unwrap();

        if contacts.len() >= MAX_CONTACTS {
            return Err(WalletError::MaxContactsReached);
        }
        if contacts.contains(&contact) {
            return Err(WalletError::ContactAlreadyAdded);
        }

        contacts.push_back(contact.clone());
        env.storage().instance().set(&CONTACTS, &contacts);

        env.events().publish((EV_ADD, owner), contact);
        Ok(())
    }

    /// Remove a trusted contact. Only the owner may call this.
    pub fn remove_contact(env: Env, contact: Address) -> Result<(), WalletError> {
        let owner = Self::require_owner(&env)?;

        let mut contacts: Vec<Address> = env.storage().instance().get(&CONTACTS).unwrap();

        let pos = contacts
            .iter()
            .position(|c| c == contact)
            .ok_or(WalletError::NotATrustedContact)?;

        contacts.remove(pos as u32);
        env.storage().instance().set(&CONTACTS, &contacts);

        env.events().publish((EV_REMOVE, owner), contact);
        Ok(())
    }

    /// Update the approval threshold. Only the owner may call this.
    pub fn set_threshold(env: Env, new_threshold: u32) -> Result<(), WalletError> {
        let owner = Self::require_owner(&env)?;

        if new_threshold == 0 {
            return Err(WalletError::InvalidThreshold);
        }

        let contacts: Vec<Address> = env.storage().instance().get(&CONTACTS).unwrap();
        if new_threshold > contacts.len() {
            return Err(WalletError::TooFewContacts);
        }

        env.storage().instance().set(&THRESHOLD, &new_threshold);

        env.events().publish((EV_THRESH, owner), new_threshold);
        Ok(())
    }

    // ── Recovery flow ──────────────────────────────────────────────────────────

    /// A trusted contact proposes (or votes for) a new owner.
    ///
    /// The first contact opens the session and casts the first vote.
    /// Subsequent contacts add their vote. Ownership transfers automatically
    /// once the threshold is reached.
    pub fn propose_recovery(
        env: Env,
        caller: Address,
        new_owner: Address,
    ) -> Result<(), WalletError> {
        caller.require_auth();

        let contacts: Vec<Address> = env.storage().instance().get(&CONTACTS).unwrap();
        if !contacts.contains(&caller) {
            return Err(WalletError::NotATrustedContact);
        }

        if env.storage().instance().has(&RECOVERY) {
            let current: Address = env.storage().instance().get(&RECOVERY).unwrap();
            if current != new_owner {
                return Err(WalletError::RecoveryAlreadyOpen);
            }
        } else {
            env.storage().instance().set(&RECOVERY, &new_owner);
            env.storage().instance().set(&VOTES, &Map::<Address, bool>::new(&env));
        }

        let mut votes: Map<Address, bool> = env.storage().instance().get(&VOTES).unwrap();
        if votes.contains_key(caller.clone()) {
            return Err(WalletError::AlreadyVoted);
        }

        votes.set(caller.clone(), true);
        env.storage().instance().set(&VOTES, &votes);

        env.events().publish((EV_VOTE, caller), new_owner.clone());

        let threshold: u32 = env.storage().instance().get(&THRESHOLD).unwrap();
        if votes.len() >= threshold {
            Self::execute_recovery(&env, new_owner);
        }

        Ok(())
    }

    /// Cancel an open recovery session. Only the current owner may do this.
    pub fn cancel_recovery(env: Env) -> Result<(), WalletError> {
        let owner = Self::require_owner(&env)?;

        if !env.storage().instance().has(&RECOVERY) {
            return Err(WalletError::NoRecoveryPending);
        }

        env.storage().instance().remove(&RECOVERY);
        env.storage().instance().remove(&VOTES);

        env.events().publish((EV_CANCEL, owner), true);
        Ok(())
    }

    // ── Read-only helpers ──────────────────────────────────────────────────────

    pub fn get_owner(env: Env) -> Address {
        env.storage().instance().get(&OWNER).unwrap()
    }

    pub fn get_threshold(env: Env) -> u32 {
        env.storage().instance().get(&THRESHOLD).unwrap()
    }

    pub fn get_contacts(env: Env) -> Vec<Address> {
        env.storage().instance().get(&CONTACTS).unwrap()
    }

    pub fn get_pending_recovery(env: Env) -> Option<Address> {
        env.storage().instance().get(&RECOVERY)
    }

    pub fn get_vote_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get::<Symbol, Map<Address, bool>>(&VOTES)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    pub fn has_voted(env: Env, contact: Address) -> bool {
        env.storage()
            .instance()
            .get::<Symbol, Map<Address, bool>>(&VOTES)
            .map(|v| v.contains_key(contact))
            .unwrap_or(false)
    }

    // ── Internal helpers ───────────────────────────────────────────────────────

    fn require_owner(env: &Env) -> Result<Address, WalletError> {
        let owner: Address = env.storage().instance().get(&OWNER).unwrap();
        owner.require_auth();
        Ok(owner)
    }

    fn execute_recovery(env: &Env, new_owner: Address) {
        let old_owner: Address = env.storage().instance().get(&OWNER).unwrap();
        env.storage().instance().set(&OWNER, &new_owner);
        env.storage().instance().remove(&RECOVERY);
        env.storage().instance().remove(&VOTES);
        env.events().publish((EV_RECOVER, old_owner), new_owner);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, SocialRecoveryWalletClient<'static>, Address) {
        let env    = Env::default();
        env.mock_all_auths();
        let id     = env.register_contract(None, SocialRecoveryWallet);
        let client = SocialRecoveryWalletClient::new(&env, &id);
        let owner  = Address::generate(&env);
        client.initialize(&owner, &2).unwrap();
        (env, client, owner)
    }

    #[test]
    fn test_add_and_list_contacts() {
        let (env, client, _) = setup();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        client.add_contact(&bob).unwrap();
        assert_eq!(client.get_contacts().len(), 2);
    }

    #[test]
    fn test_remove_contact() {
        let (env, client, _) = setup();
        let alice = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        client.remove_contact(&alice).unwrap();
        assert_eq!(client.get_contacts().len(), 0);
    }

    #[test]
    fn test_full_recovery_flow() {
        let (env, client, _) = setup();
        let alice     = Address::generate(&env);
        let bob       = Address::generate(&env);
        let new_owner = Address::generate(&env);

        client.add_contact(&alice).unwrap();
        client.add_contact(&bob).unwrap();

        client.propose_recovery(&alice, &new_owner).unwrap();
        assert_eq!(client.get_vote_count(), 1);
        assert!(client.get_pending_recovery().is_some());

        client.propose_recovery(&bob, &new_owner).unwrap();
        assert_eq!(client.get_owner(), new_owner);
        assert!(client.get_pending_recovery().is_none());
    }

    #[test]
    fn test_cancel_recovery() {
        let (env, client, _) = setup();
        let alice     = Address::generate(&env);
        let new_owner = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        client.propose_recovery(&alice, &new_owner).unwrap();
        client.cancel_recovery().unwrap();
        assert!(client.get_pending_recovery().is_none());
    }

    #[test]
    fn test_set_threshold() {
        let (env, client, _) = setup();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        client.add_contact(&bob).unwrap();
        client.set_threshold(&1).unwrap();
        assert_eq!(client.get_threshold(), 1);
    }

    #[test]
    fn test_has_voted() {
        let (env, client, _) = setup();
        let alice     = Address::generate(&env);
        let new_owner = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        assert!(!client.has_voted(&alice));
        client.propose_recovery(&alice, &new_owner).unwrap();
        assert!(client.has_voted(&alice));
    }

    #[test]
    fn test_non_contact_cannot_vote() {
        let (env, client, _) = setup();
        let stranger  = Address::generate(&env);
        let new_owner = Address::generate(&env);
        assert!(client.try_propose_recovery(&stranger, &new_owner).is_err());
    }

    #[test]
    fn test_double_vote_rejected() {
        let (env, client, _) = setup();
        let alice     = Address::generate(&env);
        let new_owner = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        client.propose_recovery(&alice, &new_owner).unwrap();
        assert!(client.try_propose_recovery(&alice, &new_owner).is_err());
    }

    #[test]
    fn test_duplicate_contact_rejected() {
        let (env, client, _) = setup();
        let alice = Address::generate(&env);
        client.add_contact(&alice).unwrap();
        assert!(client.try_add_contact(&alice).is_err());
    }

    #[test]
    fn test_cannot_initialize_twice() {
        let (env, client, owner) = setup();
        assert!(client.try_initialize(&owner, &2).is_err());
    }
}