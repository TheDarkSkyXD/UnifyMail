# 💌 UnifyMail

**UnifyMail is a powerful, open-source email client.** It is built for speed and efficiency using a C++ sync engine based on [Mailcore2](https://github.com/MailCore/mailcore2), using roughly half the RAM and CPU of other Electron-based mail clients. It idles with almost zero "CPU Wakes", which translates to great battery life.

UnifyMail's UI is open source (GPLv3) and written in TypeScript with [Electron](https://github.com/atom/electron) and [React](https://facebook.github.io/react/) - it's built on a plugin architecture and was designed to be easy to extend. Check out [CONTRIBUTING.md](https://github.com/TheDarkSkyXD/UnifyMail/blob/master/CONTRIBUTING.md) to get started!

UnifyMail's sync engine is spawned by the Electron application and runs locally on your computer. [It is open source (GPLv3) and written in C++ and C.](https://github.com/TheDarkSkyXD/UnifyMail-Sync) For convenience, however, when you set up your development environment, UnifyMail uses the latest version of the sync engine we've shipped for your platform so you don't need to pull sources or install its compile-time dependencies.

![UnifyMail Screenshot](https://github.com/TheDarkSkyXD/UnifyMail/raw/master/screenshots/hero_graphic_mac%402x.png)

## Features

UnifyMail comes packed with powerful features like Unified Inbox, Snooze, Send
Later, Mail Rules, Templates and more. **All of these features run in the client - UnifyMail does not send
your email credentials to the cloud.** For more information, check out the [GitHub Repository](https://github.com/TheDarkSkyXD/UnifyMail).

## Download UnifyMail

You can download compiled versions of UnifyMail for Windows, Mac OS X, and
Linux (deb, rpm and snap) from
[GitHub Releases](https://github.com/TheDarkSkyXD/UnifyMail/releases).

## Getting Help

You can find community-based help and discussion with other UnifyMail users on our
[GitHub Discussions](https://github.com/TheDarkSkyXD/UnifyMail/discussions).

## Contributing

UnifyMail is entirely open-source. Pull requests and contributions are
welcome! There are three ways to contribute: building a plugin, building a
theme, and submitting pull requests to the project itself. When you're getting
started, you may want to join our
[Discussions](https://github.com/TheDarkSkyXD/UnifyMail/discussions) so you can ask questions and
learn from other people doing development.

[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-v2.0%20adopted-ff69b4.svg)](CODE_OF_CONDUCT.md)

### Running UnifyMail from Source

To install all dependencies and run UnifyMail from its source code,
run the following commands from the root directory of the UnifyMail repository:

```
export npm_config_arch=x64 # If you are on an M1 / Apple Silicon Mac
npm install
npm start
```

You can attach command line parameters by separating them using a double hyphen:

```
npm start -- --help
```

### Building UnifyMail

To build UnifyMail, you need to run the following command from the root directory
of the UnifyMail repository:

```
npm run-script build
```

### Building A Plugin

Plugins lie at the heart of UnifyMail and give it its powerful features.
Building your own plugins allows you to integrate the app with other tools,
experiment with new workflows, and more. Follow the [Getting Started
guide](https://TheDarkSkyXD.github.io/UnifyMail/) to write your first plugin in
five minutes.

- To create your own theme, check out the
  [UnifyMail-Theme-Starter](https://github.com/TheDarkSkyXD/UnifyMail-Theme-Starter).

- To create your own plugin, check out the
  [UnifyMail-Plugin-Starter](https://github.com/TheDarkSkyXD/UnifyMail-Plugin-Starter).

A plugin "store" is planned for the future to make it
easy for other users to discover plugins you create. (Right now, users need to
"sideload" the plugins into the app by downloading them and copying them into
place.)

You can share and browse UnifyMail Plugins, and discuss plugin development
with other developers, on our
[Discussions](https://github.com/TheDarkSkyXD/UnifyMail/discussions).

### Building a Theme

The UnifyMail user interface is styled using CSS, which means it's easy to
modify and extend. UnifyMail comes stock with a few beautiful themes, and
there are many more which have been built by community developers. To start
creating a theme, [clone the theme starter](https://github.com/TheDarkSkyXD/UnifyMail-Theme-Starter)!

You can share and browse UnifyMail Themes, and discuss theme development with other developers, on our [Discussions](https://github.com/TheDarkSkyXD/UnifyMail/discussions).

### Localizing / Translating

UnifyMail (1.5.0 and above) supports localization. If you're a fluent speaker of
another language, we'd love your help improving translations. Check out the
[LOCALIZATION](https://github.com/TheDarkSkyXD/UnifyMail/blob/master/LOCALIZATION.md)
guide for more information. You can discuss localization and translation with
other developers on our [Discussions](https://github.com/TheDarkSkyXD/UnifyMail/discussions).

### Contributing to UnifyMail Core

Pull requests are always welcome - check out
[CONTRIBUTING](https://github.com/TheDarkSkyXD/UnifyMail/blob/master/CONTRIBUTING.md)
for more information about setting up the development environment, running
tests locally, and submitting pull requests.
