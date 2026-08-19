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
[![LinkedIn][linkedin-shield]][linkedin-url]

<!-- PROJECT LOGO -->
<br />
<div align="center">
  <a href="https://github.com/bitrealm-dev/message-vault">
    <img src="images/logo.png" alt="Logo" width="80" height="80">
  </a>

<h3 align="center">Message Vault</h3>

  <p align="center">
    project_description
    <br />
    <a href="https://bitrealm.dev/"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="https://github.com/bitrealm-dev/message-vault">View Demo</a>
    &middot;
    <a href="https://github.com/bitrealm-dev/message-vault/issues/new?labels=bug&template=bug_report.md">Report Bug</a>
    &middot;
    <a href="https://github.com/bitrealm-dev/message-vault/issues/new?labels=enhancement&template=feature-request---.md">Request Feature</a>
  </p>
</div>



<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

# Message Vault

![Logo](docs/message-vault.jpg)

Extract messages from phone backups, import them into a local vault, and browse them in a website you control.

## What it is

Message Vault has two parts that run on a machine you control:

- **The vault** — a Docker container with a REST API and a SQLite database. It stores your messages and serves them through a website in your browser.
- **The desktop app** — a program that extracts messages from Apple and Android phone backups, converts them between formats, and imports them into the vault.

There is no cloud account. Messages are not uploaded to a Message Vault service. The vault you run has a local login (the demo user, or an account you create).

## Who it is for

People who have phone backups and want to extract, convert, and browse those messages locally.


## What you can do

- **Extract** Apple Messages (`chat.db` or an iPhone backup), Android SMS/MMS from SMS Backup & Restore XML, and WhatsApp. GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+ are limited rescue imports for files you already have.
- **Convert** an existing Message Vault folder between JSON Lines, JSON, CSV, EML, MBOX, and XML.
- **Import, browse, and export** using the desktop app and the vault.

Full guide: **https://bitrealm.dev/**

Converter and mapping details: [Formats](https://bitrealm.dev/formats/) (Developer).

## Project description

{The README template guide includes information on how to write a project description and a project description. Here are some examples of effective phrases for describing a project.}

With *{Project Name}* you can *{verb}* *{noun}*...

*{Project Name}* helps you *{verb}* *{noun}*...

Unlike *{alternative}*, *{Project Name}* *{verb}* *{noun}*...

{Include screenshots and/or demo videos if applicable}

## Who this project is for

This project is intended for {target user} who wants to {user objective}.


## About The Project

[![Product Name Screen Shot][product-screenshot]](https://example.com)

Here's a blank template to get started. To avoid retyping too much info, do a search and replace with your text editor for the following: `github_username`, `repo_name`, `twitter_handle`, `linkedin_username`, `email_client`, `email`, `project_title`, `project_description`, `project_license`


### Built With

[![Rust][Rust-dev]][Rust-url] [![React][React.js]][React-url] [![Tauri][Tauri]][Tauri-url] [![Vite][Vite]][Vite-url] [![SQLite][SQLite]][SQLite-url] [![Docker][Docker]][Docker-url]


## Getting Started

See tutorial docs for a basic setup and run, or see contributing.md to setup a local dev environment and compile from source.

This is an example of how you may give instructions on setting up your project locally.
To get a local copy up and running follow these simple example steps.

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## Additional documentation

Complete documentation, user guide and tutorial bitrealm.dev
developer and architecture documentation in /docs 

## How to get help

{Include links and brief descriptions for support resources. Examples provided in README template guide.}

- Reference link 1
- Reference link 2
- Reference link 3...

## License

Distributed under the Fair Core License. See [LICENSE](LICENSE) for more information.

## Project Status

This project is currently under heavy development and moving towards a v1.0.0 release.

<!-- CONTACT -->
## Maintainers

Matt Beisser - [message.vault@bitrealm.dev](message.vault@bitrealm.dev)

## Related Projects

- https://www.openextract.app/
- https://imazing.com/
- https://github.com/jberkel/sms-backup-plus
- https://www.synctech.com.au/sms-backup-restore/
- https://www.openmobilealliance.org/specifications/affiliates/wap-forum
- https://github.com/ReagentX/imessage-exporter
- https://github.com/tyrrrz/discordchatexporter
- https://discordexport.com/discord-user-list/
- https://github.com/kenn-io/msgvault
- https://github.com/ChatLab/ChatLab


<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->
[contributors-shield]: https://img.shields.io/github/contributors/github_username/repo_name.svg?style=for-the-badge
[contributors-url]: https://github.com/github_username/repo_name/graphs/contributors
<!-- [forks-shield]: https://img.shields.io/github/forks/github_username/repo_name.svg?style=for-the-badge
[forks-url]: https://github.com/github_username/repo_name/network/members
[stars-shield]: https://img.shields.io/github/stars/github_username/repo_name.svg?style=for-the-badge
[stars-url]: https://github.com/github_username/repo_name/stargazers -->
[issues-shield]: https://img.shields.io/github/issues/github_username/repo_name.svg?style=for-the-badge
[issues-url]: https://github.com/github_username/repo_name/issues
[license-shield]: https://img.shields.io/github/license/github_username/repo_name.svg?style=for-the-badge
[license-url]: https://github.com/github_username/repo_name/blob/master/LICENSE.txt
[linkedin-shield]: https://img.shields.io/badge/-LinkedIn-black.svg?style=for-the-badge&logo=linkedin&colorB=555
[linkedin-url]: https://linkedin.com/in/linkedin_username
[product-screenshot]: images/screenshot.png

<!-- Shields.io badges. You can a comprehensive list with many more badges at: https://github.com/inttter/md-badges -->

[React.js]: https://img.shields.io/badge/React-20232A?style=for-the-badge&logo=react&logoColor=61DAFB
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

> Like this `README.md`? Use the [Best-README-Template][https://github.com/othneildrew/Best-README-Template] in your repo, and don't forget to explore other templates from [The Good Docs Project](https://thegooddocsproject.dev/).
