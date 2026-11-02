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
