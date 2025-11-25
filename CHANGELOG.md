## [0.5.0] - 2025-11-25

### 🚀 Features

- *(app)* Emoji auto complete (#831)
- *(app)* Allow group chat profile updates (#836)
- *(app)* Image attachment upload progress (#833)
- *(app)* Image attachment upload cancellation and retry (#834)
- *(app)* Open member details in more places (#840)
- *(app)* Apply design specs to onboarding (#837)
- *(coreclient)* Connection requests via targeted messages (#846)
- *(app)* Image viewer improvements (#838)
- *(app)* Support uploading files and images from camera (#843)
- *(app)* Save attachment context menu (#844)
- *(app)* Adjust design of user profile screen (#849)
- *(app)* Design consistency (#850)
- *(app)* Add upload confirmation screen (#855)
- *(backend)* Add CheckHandleExists endpoint (#857)
- *(app)* Update text in various places (#859)

### 🐛 Bug Fixes

- *(ci)* Fix typo in product shot content (#839)
- *(backend)* Verify app message signatures on DS (#841)
- *(app)* Composer no longer shown in inactive chats (#842)
- *(app)* Newly created chats appear on top of the chat list (#845)

### ⚙️ Miscellaneous Tasks

- *(build)* Replace dart tools by rust tools (#830)
- *(app)* Upgrade flutter to 3.38.1 and dart to 3.10.0 (#832)
- *(coreclient)* Add sanity checks when adding contact by handle (#847)
- *(coreclient)* Add sanity checks when adding contact from group (#848)

## [0.4.0] - 2025-11-14

### 🚀 Features

- *(server)* Add max attachment size limit (#811)
- *(server)* Add server configuration to enable/disable post policy uploads (#812)
- *(server)* Add content length to attachment provisioning (#813)
- *(app)* New onboarding flow (#817)
- *(app)* Zoomable and pannable image viewer (#820)

### 🐛 Bug Fixes

- *(server)* Suppress disconnect errors in queues (#808)
- *(server)* Stale qs listeners are not cleaned up (#814)
- *(app)* Use localized date and time (#815)
- *(app)* Query in scheduled KeyPackage uploads (#816)
- *(app)* Fix mark as read datetime truncation corner case (#818)
- *(app)* Various navigation issues (#819)
- *(app)* Messages are no longer marked as read when the desktop is in the background (#821)
- *(app)* Default UI scale (#823)
- *(app)* Don't show processing errors in notifications (#826)
- *(app)* Constrain long chat names (#827)

### 💼 Other

- Tool to prune unused UI text strings in ARB files (#824)

### ⚙️ Miscellaneous Tasks

- Enable scraping metrics (#807)
- Add merge group trigger to required actions (#822)
- *(app)* L10n improvements (#825)

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
