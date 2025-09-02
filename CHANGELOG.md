# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.12.0] - 2025-09-02

### Added
- **User-to-user communication**: New gift wrap messaging system for direct user communication
  - `dmtouser` command: Send gift wrapped messages between users using trade keys from order IDs
  - `getdmuser` command: View gift wrapped messages received on trade keys and admin key
- **Admin communication**: Enhanced admin messaging capabilities
  - `admsenddm` command: Admin can send gift wrapped messages to users
  - `getadmindm` command: Get messages sent to mostro key for admin review
- **Data recovery**: New `restore` command for restoring user data
- **Admin tooling**: More specific `AdmAddSolver` command for better admin dispute handling

### Changed
- **Payment method handling**: Improved payment method processing and validation
- **Database operations**: Optimized database queries with simplified DISTINCT trade_keys
- **Database migrations**: Enhanced migration system to properly handle schema updates
- **Keys management**: Improved key rotation system, removed nsec parameter dependency
- **Code quality**: Extensive code refactoring for more idiomatic Rust patterns
- **Dependencies**: Updated to Nostr SDK 0.40 and latest mostro-core versions
- **Message filtering**: Enhanced DM filtering - `getdm` shows only Mostro messages, `getdmuser` shows only user messages
- **Dispute display**: Improved dispute information display without Option wrapper
- **Premium validation**: Allow hyphen values in premium calculations

### Fixed
- **Order canceling**: Fixed issues with order cancellation process
- **Trade key creation**: Fixed incorrect check for sell orders with range that was only working for specific order types
- **Error handling**: Improved error handling in order response processing
- **Database consistency**: Fixed INSERT placeholder count issues
- **Admin keys**: Fixed admin key sending in dispute command scenarios
- **Timestamp handling**: Fixed database operations to not update created_at on UPDATE operations
- **Keys management**: Resolved various issues related to keys management and rotation

### Removed
- **Token system**: Removed seller/buyer token functionality with database migration to drop token columns
- **Trade public key**: Removed tradepubkey functionality 
- **nsec parameter**: Removed nsec parameter in favor of improved keys rotation system
- **Identity requirement**: Removed requirement for identity pubkey when sending messages (secret mode support)
