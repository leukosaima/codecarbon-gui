# Installing CodeCarbon Desktop

No terminal needed. Download the file for your computer from the
[Releases page](https://github.com/mlco2/codecarbon-gui/releases)
(or, for development builds, from the *Artifacts* section of a run on the
[Actions page](https://github.com/mlco2/codecarbon-gui/actions)) and follow
the steps for your system.

The app is not yet signed with a Microsoft or Apple developer certificate, so
each system shows a one-time warning the first time you open it. This is
expected; the steps below explain how to get past it. Code signing is on the
[roadmap](ROADMAP.md).

## Windows

1. Download `CodeCarbon_<version>_x64-setup.exe` (or the `.msi`).
2. Double-click it. If Windows SmartScreen says *"Windows protected your PC"*,
   click **More info**, then **Run anyway**.
3. Follow the installer. CodeCarbon starts automatically and appears as a
   small leaf icon in the notification area (bottom-right, next to the clock —
   click the `^` arrow if it is hidden).
4. Click the icon to open the panel, then **Start measuring**.

To uninstall: *Settings → Apps → CodeCarbon → Uninstall*.

## macOS

1. Download the `.dmg` that matches your Mac:
   `…_aarch64.dmg` for Apple Silicon (M1 and newer), `…_x64.dmg` for Intel.
   (Not sure? Apple menu → *About This Mac*: "Chip: Apple M…" means Apple
   Silicon.)
2. Open the `.dmg` and drag **CodeCarbon** into **Applications**.
3. First launch: macOS will say the app *"cannot be opened because Apple
   cannot check it for malicious software"* or *"is from an unidentified
   developer"*.
   * macOS 15 (Sequoia) and newer: open **System Settings → Privacy &
     Security**, scroll down to the message about CodeCarbon and click
     **Open Anyway**, then confirm.
   * macOS 14 and older: in Finder, **right-click** (or Control-click) the app
     and choose **Open**, then **Open** again.
4. CodeCarbon has no Dock icon: look for the leaf in the **menu bar** (top
   right). Click it to open the panel, then **Start measuring**. While
   measuring, the current power draw is shown next to the icon.

To uninstall: drag the app from *Applications* to the Trash.

## Linux

Pick the format your distribution supports:

* **AppImage** (works everywhere): download `…_amd64.AppImage`, right-click
  it → *Properties* → *Permissions* → tick *Allow executing file as program*
  (or *Executable as Program*), then double-click it.
* **Debian / Ubuntu / Mint**: download the `.deb` and open it with your
  software centre (or double-click), then launch *CodeCarbon* from your
  application menu.
* **Fedora / openSUSE**: same with the `.rpm`.

The app lives in the system tray. On **GNOME** the tray is hidden by default:
install the *AppIndicator and KStatusNotifierItem Support* extension (from
your distribution's *Extensions* app or
[extensions.gnome.org](https://extensions.gnome.org/extension/615/appindicator-support/))
and the icon will appear in the top bar. KDE, XFCE, Cinnamon and MATE show
tray icons out of the box.

## Updates

Newer versions announce themselves: a banner on the panel's first tab
("CodeCarbon 0.2.0 is available — **Install**") and an *Update to…* item
in the tray menu. Installing stops a running measurement first (it is
saved), then the app restarts in the new version. Updates are only
installed if they are signed by the CodeCarbon Desktop project. You can
turn off the daily check in *Settings → Updates*, and check by hand in
*About*. Versions released before automatic updates were switched on
cannot update themselves: download the new version once from the Releases
page. On Linux, `.deb` / `.rpm` updates ask for your password; the
AppImage updates in place.

## Checking that your download is genuine (optional)

Because the installers are not code-signed yet, each release also publishes
`SHA256SUMS.txt` and a build-provenance attestation made by GitHub when the
files were built from the tagged source. If you are the careful type:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing            # macOS: shasum -a 256 -c
gh attestation verify CodeCarbon_0.1.0_aarch64.dmg -R mlco2/codecarbon-gui
```

See [SECURITY.md](../SECURITY.md) for what the app does and does not do on
your computer.

## Connecting to the CodeCarbon dashboard (optional)

*Settings → CodeCarbon dashboard → Sign in or create an account* opens your
browser on the free CodeCarbon dashboard. Once signed in, pick (or create)
an organisation, a project and an experiment, and switch on *Upload my
sessions*. Every session you stop from then on appears on
<https://dashboard.codecarbon.io>. You can sign out at any time; nothing
is uploaded unless this is switched on.

## History

The *History* tab shows your totals for today, this week and all time, a
30-day chart, everyday equivalents (kilometres by car, smartphone charges…)
and the list of past sessions. It reads the same `emissions.csv` file
CodeCarbon writes.

## Language

The app follows your system language (English or French). Change it in
*Settings → Language*.

## What do the numbers mean?

* **Power (W)** — how much electricity your machine is drawing right now,
  split into CPU, GPU and memory.
* **Energy (Wh)** — power accumulated since you pressed Start.
* **CO₂e (g)** — the energy multiplied by the carbon intensity of your
  country's electricity (CodeCarbon's data; the country is detected from your
  IP address unless you set it in *Settings*).

Open **"How is this measured?"** in the panel to see which method CodeCarbon
is using on your machine. On most laptops the CPU figure is an *estimate*
derived from CPU load and the processor's rated power (TDP); real
measurements (Intel RAPL on Linux, Windows Energy Meter) are used when the
system exposes them. See the
[CodeCarbon methodology](https://mlco2.github.io/codecarbon/methodology.html).

## Where is my data?

Every session is appended to `emissions.csv` in the app's data folder
(a `measurements` folder inside the app's data directory)
(menu → **Open data folder**). It is the same CSV format the CodeCarbon
Python package writes.
