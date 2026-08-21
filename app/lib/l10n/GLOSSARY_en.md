# Localization Glossary

This glossary provides definitions for key terms used in Air's user interface to ensure consistent translation across all supported languages.

The template in `app_en.arb` decides what a string says. This glossary decides which term it says it with. Where the two disagree, flag it rather than picking silently.

## Variety

The template is US English. "Licenses" carries the US spelling rather than "Licences".

A language variant gets its own files instead of being mixed into the base language. Adding British English would mean `app_en_GB.arb` alongside `GLOSSARY_en_GB.md`, not GB spellings inside these.

## Register

English addresses the user in the second person, "you".

Every locale has one register. Each one is set in that locale's own glossary and is not varied per string.

## Capitalization

UI labels use sentence case. Only the first word and proper nouns take a capital, so "Invite codes" and "Safety code" rather than "Invite Codes" and "Safety Code". Names keep their capitals, including Air, key names like Enter, and document titles like Terms of Use.

Translations follow their own orthography instead of copying English capitalization. German capitalizes every noun. French and Swedish capitalize less than English does.

## Core App Terms

| Term | Definition | Context |
|------|------------|---------|
| **Air** | The name of the application | Always keep as "Air", do not translate |
| **Account** | A user's Air account | Appears as "Air account". "Air" survives in compounds |
| **Username** | A unique identifier that users can share to connect with others | Only used for connections |
| **Display Name** | The name that appears in chats and is visible to other users | Different from username. This is what people see when you message them |
| **Chat** | A chat thread between two or more people | Between two people or in a group |
| **Contact** | Another user you can message | Appears as "Air contact" |
| **Member** | A participant in a group chat | Used in group chat contexts |

## Messaging Terms

| Term | Definition | Context |
|------|------------|---------|
| **Message** | A text, image, or file sent in a chat | |
| **Draft** | An unsent message that has been typed but not sent yet | Appears in chat list |
| **Attachment** | A file or image sent with a message | |
| **Compose** | To write a new message | |
| **Edit** | To modify a message that has already been sent | |
| **Reply** | To respond to a specific message in a chat | |
| **React** | To attach an emoji to a message | Message context menu action |
| **Emoji** | A single pictographic character | Used in the reaction picker |
| **Group** | A chat with multiple participants | |

## Actions & Interface

| Term | Definition | Context |
|------|------------|---------|
| **Connect** | To add someone as a contact using their username | Initial action to start messaging someone new. Not the same as Link |
| **Add** | To include someone in a group or add them to contacts | |
| **Remove** | To take someone out of a group chat | Use "remove" not "delete" for people |
| **Delete** | To permanently remove content (messages, files, etc.) | Use "delete" for content, "remove" for people |
| **Leave** | To take yourself out of a group | Unlike Remove, which acts on someone else |
| **Block** | To stop receiving messages from someone | |
| **Unblock** | To reverse a block | One word per locale, used in buttons and body text alike |
| **Mute** | To stop notifications for a chat | |
| **Unmute** | To reverse a mute | |
| **Link** | To give another device access to your account | Devices, not contacts. Not the same as Connect |
| **Unlink** | To revoke a linked device's access | |
| **Report Spam** | To flag a user or message as unwanted/spam | Moderation feature |

## File & Data Terms

| Term | Definition | Context |
|------|------------|---------|
| **Byte Units** | File size measurements (B, KB, MB, GB, etc.) | Localize where the language uses different units |
| **Attachment Size** | The file size of uploaded content | |
| **Upload** | To send a file or image | Keep distinct from downloading, which no current string covers |

## Status & Time

| Term | Definition | Context |
|------|------------|---------|
| **Now** | Current moment in time | Timestamp for very recent messages |
| **Yesterday** | The day before today | Timestamp for messages from yesterday |
| **Sending** | Message is on its way to the server | Message status indicator |
| **Failed to send** | Message could not be sent | Message status indicator |
| **Sent** | Message has been delivered to the server | Message status indicator |
| **Delivered** | Message has been delivered to the recipient's device | Message status indicator |
| **Read** | Message has been seen by the recipient | Message status indicator |
| **Read receipts** | The setting that controls whether read status is shared | Settings toggle |
| **Edited** | Indicates a message has been modified after sending | Appears next to modified messages |

## Settings & Help

| Term | Definition | Context |
|------|------------|---------|
| **Settings** | App configuration options | The screen is "Profile and settings" |
| **Profile** | User's personal information and preferences | |
| **Safety code** | A code two contacts compare to confirm their chat is not intercepted | Contact profile row and its own pane |
| **Invite code** | A code someone needs to join Air | Always "invite code", never "invitation code" |
| **Linked devices** | The other devices signed in to the account | Settings section |
| **Server** | The host an account lives on | Chosen during sign up and during linking |
| **Help** | Support and assistance section | |
| **Contact Air** | Support contact options | |
| **Licenses** | Legal information about open source components | |
| **Version Info** | Technical details about the app version | |

## Notes for Translators

- **Air** should never be translated, it is the product name, and it stays in compounds like "Air account"
- **Username** vs **Display Name**: Username is for finding people, Display Name is for identification in chats
- **Remove** vs **Delete**: the English template picks the verb, remove for people and delete for content. Translations mirror whichever verb the English uses rather than reclassifying the object themselves
- **Connect** vs **Link**: Connect adds a contact, Link adds a device. Every locale keeps two distinct words for these
- **Block** and **Unblock** each get exactly one word per locale, including inside dialog body text
- Consider cultural context for messaging terminology in your language
