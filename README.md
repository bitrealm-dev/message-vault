<a id="readme-top"></a>
<!-- Template inspired by the Best-README-Template
     https://github.com/blackwell-systems/oss-kit/blob/main/templates/README.template.md
-->

<!-- PROJECT SHIELDS -->
<!--
*** Reference links are enclosed in brackets [ ] instead of parentheses ( ).
*** See the bottom of this document for the declaration of the reference variables
*** for contributors-url, forks-url, etc. This is an optional, concise syntax you may use.
*** https://www.markdownguide.org/basic-syntax/#reference-style-links
-->

<!-- 
[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
-->
[![Issues][issues-shield]][issues-url]
[![project_license][license-shield]][license-url]

<!-- PROJECT LOGO -->
<br />
<div align="center">
  <a href="https://github.com/bitrealm-dev/message-vault">
    <img src="images/logo.png" alt="Logo" width="80" height="80">
  </a>

<h3 align="center">Message Vault</h3>

  <p align="center">
    Keep your text messages on a computer you control.
    <br />
    <a href="https://bitrealm.dev/"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="https://github.com/bitrealm-dev/message-vault">View Demo</a>
    &middot;
    <a href="https://github.com/bitrealm-dev/message-vault/issues/new?labels=bug&template=bug_report.md">Report Bug</a>
    &middot;
    <a href="https://github.com/bitrealm-dev/message-vault/issues/new?labels=enhancement&template=feature_request.md">Request Feature</a>
  </p>
</div>

- [Message Vault](#message-vault)
  - [About The Project](#about-the-project)
  - [Who The Project Is For](#who-the-project-is-for)
  - [Getting Started](#getting-started)
  - [Contributing](#contributing)
  - [Additional documentation](#additional-documentation)
  - [License](#license)
  - [Project Status](#project-status)
  - [Maintainers](#maintainers)
  - [Related Projects](#related-projects)

# Message Vault

<p align="center">
  <img src="docs/img/message-vault.jpg" width="50%" />
</p>

Pry digitial conversations out of apps and store them in your own self-hosted vault.

## About The Project

[![Docker][Docker]][Docker-url] [![React][React.js]][React-url] [![Rust][Rust-dev]][Rust-url] [![SQLite][SQLite]][SQLite-url] [![Tauri][Tauri]][Tauri-url] [![Vite][Vite]][Vite-url]

Chat apps make sending a message easy. They make owning that message hard.

You already paid for the phone. You already pay for the plan. Many people also pay for extra cloud storage so chats are “backed up.” That backup usually only works one way. You can put it back onto a phone the company still controls. You often cannot open it on a computer, search years of history, or take it with you when you switch apps.

That is not how email works. You can download every message. You can change providers. You can keep a copy on your computer and still read it in any mail program. The mail is yours.

Texts should work the same way. They do not.

This project started after I left Google’s mail service. I merged three old email addresses into one new inbox. I deleted years of junk. What was left was a clean record of twenty-two years of contacts and mail. I wanted the same thing for my texts: one place, one history, and no leftover accounts.

Getting there was a mess. Old Android phones. Apple Messages on a Mac. WhatsApp on top of that. Each app kept its own copy and had its own rules. Some backups only worked if you restored them onto a phone. Some exports were missing pieces. Some files were almost impossible to read. There was no simple “download all my messages” button.

Message Vault is the tool that was missing then.

A vault here means a private store of your messages on a computer you control. Nothing is sent to a Message Vault company server. There is no Message Vault cloud account. You sign in on your own machine.

The software has two parts:

- **The vault** — the backed that runs on your computer. Sign in, read conversations, search, and look at contacts and photos.
- **The desktop app** — a program on the same computer. Point it at a backup you already made. It reads the chats and puts them into the vault.

The website is enough to look around. Putting your own messages in needs the desktop app and a backup file or folder from your phone.

You can bring in:

- Apple Messages from an iPhone backup, or from Messages on a Mac
- Android texts and picture messages from an SMS Backup & Restore file
- WhatsApp from an iPhone backup or from WhatsApp’s Android files

A few older export files can still be brought in if that is all you have left.

Once messages are in the vault you can:

- Read threads the way you would on a phone, including group chats
- Search across years of conversations
- Keep photos, videos, and other attachments with the messages
- Save a copy back out as ordinary files if you want a folder on disk
- Combine texts from more than one phone or app into one archive

Full guide: **https://bitrealm.dev/**

## Who The Project Is For

This project is for people who want a personal copy of their phone messages. That includes anyone replacing a phone, leaving a chat app, or keeping a long-term archive of texts.

## Getting Started

Follow the [User Guide](https://bitrealm.dev/get-started/what-is-message-vault/) to run the demo and import your own data.

See [CONTRIBUTING.md](CONTRIBUTING.md) to setup a local dev environment and compile and run from source.

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also [open a feature request](https://github.com/bitrealm-dev/message-vault/issues/new?labels=enhancement&template=feature_request.md).
Don't forget to give the project a star! Thanks again!

## Additional documentation

See [https://bitrealm.dev] for the [User Guide](https://bitrealm.dev) and [Developer Details](https://bitrealm.dev/developer).

<!-- ## How to get help

{Include links and brief descriptions for support resources. Examples provided in README template guide.}

- Reference link 1
- Reference link 2
- Reference link 3... -->

## License

Distributed under the Fair Core License. See [LICENSE.md](LICENSE.md) for more information.

## Project Status

This project is currently under heavy development and moving towards a v1.0.0 release.

## Maintainers

Matt Beisser - [message.vault@bitrealm.dev](message.vault@bitrealm.dev)

## Related Projects

- [ChatLab](https://github.com/ChatLab/ChatLab) - Local-first tool that analyzes chat history with AI.
- [Discord Export](https://discordexport.com/discord-user-list) - Hosted helpers for pulling Discord data, including a server member list.
- [DiscordChatExporter](https://github.com/tyrrrz/discordchatexporter) - Exports Discord channel history to HTML, JSON, CSV, and other files.
- [iMazing](https://imazing.com/) - Manages iOS devices and can export message threads from an iPhone or iTunes backup.
- [imessage-exporter](https://github.com/ReagentX/imessage-exporter) - Command-line tool that exports Apple Messages from `chat.db` to several text formats.
- [msgvault](https://github.com/kenn-io/msgvault) - Offline archive for email and chat with search and analytics on SQLite and DuckDB.
- [OMA WAP Forum](https://www.openmobilealliance.org/specifications/affiliates/wap-forum) - Specifications for WAP and MMS that some Android SMS apps still encode.
- [OpenExtract](https://www.openextract.app/) - Pulls iMessage, SMS, photos, and voicemail out of a local iPhone backup.
- [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore) - Android app from SyncTech that writes SMS and MMS to an XML file.
- [SMS Backup+](https://github.com/jberkel/sms-backup-plus) - Android app that backs SMS and MMS up to an IMAP mailbox (often Gmail).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->
<!-- [contributors-shield]: https://img.shields.io/github/contributors/bitrealm-dev/message-vault.svg?style=for-the-badge
[contributors-url]: https://github.com/bitrealm-dev/message-vault/graphs/contributors -->
<!-- [forks-shield]: https://img.shields.io/github/forks/bitrealm-dev/message-vault.svg?style=for-the-badge
[forks-url]: https://github.com/bitrealm-dev/message-vault/network/members
[stars-shield]: https://img.shields.io/github/stars/bitrealm-dev/message-vault.svg?style=for-the-badge
[stars-url]: https://github.com/bitrealm-dev/message-vault/stargazers -->
[issues-shield]: https://img.shields.io/github/issues/bitrealm-dev/message-vault.svg?style=for-the-badge
[issues-url]: https://github.com/bitrealm-dev/message-vault/issues
[license-shield]: https://img.shields.io/badge/license-FCL_1.0-blue
[license-url]: https://github.com/bitrealm-dev/message-vault/blob/main/LICENSE.md

<!-- Shields.io badges. You can a comprehensive list with many more badges at: https://github.com/inttter/md-badges -->

[React.js]: https://img.shields.io/badge/React-%2320232a.svg?logo=react&logoColor=%2361DAFB
[React-url]: https://reactjs.org/

[Rust-dev]: https://img.shields.io/badge/Rust-%23000000.svg?e&logo=rust&logoColor=white
[Rust-url]: https://rust-lang.org/

[Tauri]: https://img.shields.io/badge/Tauri-24C8D8?logo=tauri&logoColor=fff
[Tauri-url]: https://github.com/tauri-apps/tauri

[Vite]: https://img.shields.io/badge/Vite-646CFF?logo=vite&logoColor=fff
[Vite-url]: https://vite.dev/

[SQLite]: https://img.shields.io/badge/SQLite-%2307405e.svg?logo=sqlite&logoColor=white
[SQLite-url]: https://sqlite.org/

[Docker]: https://img.shields.io/badge/Docker-2496ED?logo=docker&logoColor=fff
[Docker-url]: https://www.docker.com/

> Like this `README.md`? Don't forget to explore other templates from [The Good Docs Project](https://thegooddocsproject.dev/).
