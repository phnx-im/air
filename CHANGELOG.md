## [0.9.0] - 2026-01-17

### 🚀 Features

- *(app)* Update push tokens (#942)
- *(server)* Send errors to client when processing listen requests (#963)

### 🐛 Bug Fixes

- *(app)* Support self remove proposals (#953)
- *(app)* Adding contact fails if the requester has changed their profile (#964)
- *(app)* Use work manager for android notifications (#965)
- *(app)* Retrying attachment uploads (#959)
- *(app)* Make appbutton more robust (#968)
- *(app)* Retrigger mark as read (#971)
- *(app)* Multiple contact requests per username (#970)

### 🚜 Refactor

- *(test)* Split large integration test file (#966)

## [0.8.0] - 2026-01-12

### 🚀 Features

- *(app)* Language picker (#946)
- *(app)* Separate chat avatar (#930)

### 🐛 Bug Fixes

- *(app)* Tweak two buttons (#937)
- *(app)* Set l10n fallback (#938)
- *(app)* Make sure key package upload task always exists (#944)
- *(server)* Replace key packages on publish (#948)
- *(app)* Preserve chat history after re-joining a chat (#947)
- *(app)* Long text in remove and add person buttons (#945)
- *(app)* Delete orphaned key packages if any (#949)
- *(coreclient)* Skip lifetime validation during welcome processing (#943)
- *(ci)* Fetch all commit metadata for build number to work (#950)
- *(app)* Increase quality of rendered app icons (#952)
- *(backend)* Disable group expiration on the DS (#954)
- *(app)* Added user system message was not immediately visible (#958)

### 💼 Other

- *(app)* Use number of commits as build number (#941)

### ⚙️ Miscellaneous Tasks

- *(app)* Context menu extension (#940)
- *(app)* Redact user names in logs (#955)
- *(server)* Reuse http client in push notification provider (#956)
- *(server)* Update dependencies (#957)

## [0.7.0] - 2025-12-28

### 🚀 Features

- *(app)* Sort user lists in alphabetical order (#889)
- *(app)* Generate and display safety codes (#906)
- *(app)* Adjust design of add contact dialog (#911)
- *(app)* Better image names (#915)
- *(app)* Add contact dialog in 2 steps (#914)
- *(app)* Design tweaks for the member screen (incl. a fix) (#917)
- *(app)* Disable chat details for pending chats (#923)
- *(app)* Chat list design overhaul (#926)
- *(app)* New icon system (#912)
- *(app)* Drop read receipts (#931)
- *(app)* Localize user handle validation error messages (#934)

### 🐛 Bug Fixes

- *(ci)* Exclude RC version of xcode (#907)
- *(ci)* Fixed xcode version (#908)
- *(app)* New users have invalid domain (#909)
- *(app)* Alternative file lock (#910)
- *(app)* Send on enter (#916)
- *(app)* Colors in overlapping checks icon and group details buttons (#920)
- *(app)* Don't fail profile update for groups without a chat (#921)
- *(server)* Race condition in QS postres listen/unlisten (#919)
- *(app)* Set a picture during group creation (#922)
- *(app)* Lower minimum required iOS version for NSE (#925)
- *(app)* Clean up the composer (#924)
- *(app)* Solid color for message previews (#929)
- *(app)* Size of the app back button (#927)
- *(app)* Allow underscores in legacy names (#935)
- *(app)* Safety code design (#932)

### ⚙️ Miscellaneous Tasks

- Remove unused dependencies (#913)
- Add staging deployment configuration (#918)
- *(app)* No more underscores (#933)
- *(app)* Missing translations (#928)

## [0.6.0] - 2025-12-12

### 🚀 Features

- *(app)* Design consistency (#850)
- *(coreclient)* Have contact getter return full or partial contact (#858)
- *(app)* Add connection event system messages (#852)
- *(app)* Remove group description in group creation flow (#866)
- *(app)* Filtered notifications (#870)
- *(app)* Share attachments on iOS (#863)
- *(app)* Improve saving of attachments on Android (#864)
- *(app)* Add group title editing (#865)
- *(app)* Align contact and member details design (#875)
- *(backend)* Introduce client metadata in RPCs (#878)
- *(app)* Add chat button to contact details (#860)
- *(app)* Adjusted colors (#880)
- *(app)* More color adjustments (#883)
- *(server)* Add version requirement setting in the server (#885)
- *(app, coreclient)* Update message status when sending fails (#886)
- *(app)* Rotate symbol when message is sending (#894)
- *(app)* Improve iOS NSE (#891)
- *(app)* Scale large messages in mobile context menu (#890)
- *(app)* Graceful suspension on iOS (#888)
- *(app)* Show system messages in message preview (#893)
- *(app)* Link confirmation (#896)
- *(app)* Surface handle contact errors in UI (#897)
- *(app)* Tweak display of sending and failed to send status (#899)
- *(app)* Show update required screen for unsupported clients (#887)
- *(app, server)* Registration via invitation codes (#892)
- *(app)* Connection requests (#898)

### 🐛 Bug Fixes

- *(app)* Save attachment on Desktop (#862)
- *(app)* Close all other screens when opening a chat (#868)
- *(app)* Update message list state on an updated message (#867)
- *(app)* Sometimes re-upload button is shown for downloads (#869)
- *(app)* Default scroll physics per platform (#879)
- *(app)* Various papercuts (#874)
- *(app)* Message flight calculation (#884)
- *(app)* Fix remove button in group members screen (#895)
- *(app)* Redirect to prod server for checking invitation code (#900)

### 💼 Other

- *(server)* Strip debug symbols (#873)

### 🧪 Testing

- *(server)* Allow running integration tests with external server (#872)

### ⚙️ Miscellaneous Tasks

- *(coreclient)* Expose errors when adding contact via handle (#871)
- *(app, server)* Update rust to 1.92 (#904)

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
