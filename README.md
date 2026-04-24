# 🛡️ Social Recovery Wallet — Soroban Smart Contract

> **"Don't lose everything just because you lost your keys."**  
> A Stellar / Soroban smart contract that lets a group of trusted people help you reclaim ownership of your wallet — without any centralised authority.

---

## 📖 Project Description

Losing a private key normally means losing everything in a crypto wallet — permanently. **Social Recovery Wallet** solves that by letting the wallet owner designate a set of *trusted contacts* (friends, family, a hardware device, a second phone) who can collectively vote to transfer ownership to a new address.

The contract is built with [Soroban](https://soroban.stellar.org), Stellar's native smart-contract platform written in Rust. It runs entirely on-chain; no backend, no custodian, no single point of failure.

---

## ⚙️ What It Does

The contract models a **three-phase lifecycle**:

```
Owner sets up wallet
        │
        ▼
  Add trusted contacts  ◄──── owner can add / remove contacts at any time
        │
        ▼
  [ Normal operation ]
        │
   key is lost?
        │
        ▼
  Contact A calls propose_recovery(new_owner)   ← opens a recovery session
  Contact B calls propose_recovery(new_owner)   ← casts a vote
        │
  threshold reached?
  ───────────────────
  YES → ownership is transferred to new_owner instantly, session closed
  NO  → session stays open; more contacts can still vote
        │
   owner recovered manually?
        │
        ▼
  owner calls cancel_recovery()  ← closes the session, no transfer happens
```

### Core operations

| Function | Who can call | What it does |
|---|---|---|
| `initialize(owner, threshold)` | Deployer (once) | Sets the initial owner and approval threshold |
| `add_contact(contact)` | Owner | Adds a trusted contact (max 10) |
| `remove_contact(contact)` | Owner | Removes a trusted contact |
| `set_threshold(n)` | Owner | Changes the number of approvals required |
| `propose_recovery(caller, new_owner)` | Trusted contact | Opens a session or adds a vote; transfers when threshold is met |
| `cancel_recovery()` | Owner | Cancels an open recovery session |
| `get_owner()` | Anyone | Returns the current owner |
| `get_contacts()` | Anyone | Lists all trusted contacts |
| `get_threshold()` | Anyone | Returns the current threshold |
| `get_pending_recovery()` | Anyone | Returns the candidate new owner, if a session is open |
| `get_vote_count()` | Anyone | Returns votes cast so far |
| `has_voted(contact)` | Anyone | Checks whether an address has already voted |

---

## ✨ Features

### 🔑 Non-custodial ownership transfer
Ownership moves on-chain as soon as the threshold is reached. No multisig ceremony, no off-chain coordination, no waiting for a third party.

### 👥 Configurable trusted contacts
The owner can add up to **10 trusted contacts** and update the list at any time while the wallet is healthy.

### 🗳️ Flexible approval threshold
Set the threshold to anything from 1-of-N (fast recovery) to N-of-N (maximum security). The contract enforces that the threshold can never exceed the number of registered contacts.

### 🚫 Double-vote prevention
Each contact gets exactly one vote per recovery session. Attempting to vote twice is rejected at the contract level.

### ❌ Owner-initiated cancellation
If the owner realises they still have access, they can cancel an open recovery session before it completes — preventing a rogue contact from abusing the process.

### 🛑 Self-recovery guard
An owner cannot add themselves as a trusted contact, preventing trivial threshold bypass.

### 📢 On-chain events
Every state change (`init`, `addContact`, `rmContact`, `setThresh`, `vote`, `cancelRec`, `recovered`) emits a Soroban event so off-chain indexers and wallets can track the recovery lifecycle.

### 🧪 Full test suite
The `#[cfg(test)]` module covers:
- Adding and listing contacts
- Full happy-path recovery (threshold met → ownership transfers)
- Owner cancellation
- Non-contact vote rejection
- Double-vote rejection

---

## 🚀 Getting Started

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Install Stellar CLI
cargo install --locked stellar-cli --features opt
```

### Build

```bash
stellar contract build
# → target/wasm32-unknown-unknown/release/social_recovery_wallet.wasm
```

### Test

```bash
cargo test
```

### Deploy to Testnet

```bash
# 1. Create / fund a test account
stellar keys generate --global deployer --network testnet
stellar keys fund deployer --network testnet

# 2. Deploy the contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/social_recovery_wallet.wasm \
  --source deployer \
  --network testnet

# 3. Initialise (replace CONTRACT_ID and OWNER_ADDRESS)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --owner <OWNER_ADDRESS> \
  --threshold 2
```

### Invoke: Add a contact

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source owner-key \
  --network testnet \
  -- add_contact \
  --contact <CONTACT_ADDRESS>
```

### Invoke: Propose recovery

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source contact-key \
  --network testnet \
  -- propose_recovery \
  --caller <CONTACT_ADDRESS> \
  --new_owner <NEW_OWNER_ADDRESS>
```

---

## 🏗️ Project Structure

```
social-recovery-wallet/
├── Cargo.toml          # Rust manifest & Soroban dependencies
├── README.md           # This file
└── src/
    └── lib.rs          # Contract logic + unit tests
```

---

## 🔐 Security Considerations

| Concern | Mitigation |
|---|---|
| Colluding contacts | Threshold must be set high enough that no small subset of contacts can act alone |
| Contacts changing over time | Owner should audit the contact list regularly and remove stale addresses |
| Bricked wallet (all contacts lost) | Keep at least one hardware-device contact or off-site key as a backup signer |
| Race condition on recovery | Only one `new_owner` proposal can be open at a time; a different candidate is rejected |

---

## 📄 License

MIT © 2024 — free to use, modify, and distribute.
