## [0.3.0] - 2025-11-11

### 🚀 Features

- New icons (#768)
- Suppress push notifications for selected message types (#769)
- Outbound service and receipts queue (#770)
- Introduce inter-process locking in coreclient (#779)
- Increase stack size for background execution (#778)
- New group ux (#786)
- Make markdown links clickable (#787)
- Extract plain text links (#789)
- Allow to open http/https/mailto links in browser on Android (#791)
- Add user setting for disabling read receipts (#794)
- Add basic resync functionality (although not enabled for now) (#753)
- Add Prometheus metrics (#798)
- Scheduled KeyPackage uploads (#800)
- Message context menu (#801)
- Scheduled chat messages (#793)
- Add metrics for total/mau/dau/active users in QS (#802)

### 🐛 Bug Fixes

- Missing background logs on iOS (#766)
- Small regressions (#771)
- Out of order chat list (#780)
- Don't show sender in 1:1 chats (#783)
- Unsupported file locking on Android (#790)
- Android PNs reliability (#788)
- Chat list reordering when typing the message (#781)
- Unsupported content in PN when receiving an attachment (#792)
- Adjust link colors to be visible in light and dark mode (#795)
- Update toggle color based on state (#797)
- Clean up orphaned data before migration (#799)
- Client sequence number race test sometimes did not finish (#804)

### 🚜 Refactor

- *(app)* Remove root level chat details cubit (#774)
- Remove grpc port and add `listen` server config (#796)

### ⚡ Performance

- Load chats details from an LRU cache (#775)

### ⚙️ Miscellaneous Tasks

- Increase out-of-order tolerance (#772)
- Make ratchet tolerate skipped messages (#773)
- Perform DB operations in transactions (#765)
- Remove dbg in outbound_service (#776)
- Lint for large futures (#777)
- Make the fastlane gemfile readable by dependabot (#805)
- Dry Cargo.toml (#803)
