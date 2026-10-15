# Lodestar user guide

From nothing to an app running on your iPhone or Apple TV. Allow about ten
minutes the first time, and about thirty seconds after that.

## Before you start

You need:

* A **Mac** running macOS 13 or later (developed and tested on macOS 27, Apple silicon).
* An **Apple ID**. A free one is enough. Using a separate Apple ID just for
  sideloading is a sensible precaution.
* The **app file**: an `.ipa` for iPhone/iPad or a `.tipa` for Apple TV, built for
  the device you're installing to. An iPhone app won't run on an Apple TV, and the
  other way round. Lodestar warns you when they don't match.
* Your **device**, nearby.

Apple's free-account limits apply to everything below: **3 apps per device, a
7-day signature, and 10 new App IDs per week.**

## 1. Get Lodestar

Either download `Lodestar-<version>-arm64.zip` from
[Releases](https://github.com/asroya/lodestar/releases) and move `Lodestar.app` to
Applications, or build it yourself:

```bash
git clone https://github.com/asroya/lodestar.git
cd lodestar && ./packaging/bundle.sh && open dist/Lodestar.app
```

Release downloads aren't notarized. The first time, right-click `Lodestar.app` and
choose **Open** (or run `xattr -dr com.apple.quarantine /Applications/Lodestar.app`).

macOS will ask whether Lodestar may find devices on your local network. Choose
**Allow**; Apple TV discovery needs it.

## 2. Sign in

1. Open **Apple ID** in the sidebar (or click **Sign in** at the bottom left).
2. Enter your Apple ID and password. Tick **Remember me** if you want one-click
   refreshes; the password is then kept in your Mac's Keychain.
3. Apple sends a six-digit code to your other devices. Type it into the boxes.
   (**Text me instead** switches to SMS.)

When it works, the sidebar shows your Apple ID and team ID. macOS may ask once to
let Lodestar use the Keychain; choose **Always Allow**.

## 3. Connect a device

### iPhone or iPad

Plug it in with a cable, unlock it, and tap **Trust**. It appears under **Device**
on the Install page within a couple of seconds.

On iOS 16 and later, also turn on **Developer Mode** (Settings › Privacy &
Security); the phone restarts once.

### Apple TV 4K

If your Apple TV is already paired with this Mac, for example through Xcode's
*Devices and Simulators* window, it appears on its own, tagged **Wi-Fi**. Nothing
else to do.

To pair a TV for the first time with Lodestar itself (this path is new and hasn't
been verified on hardware yet; please report how it goes):

1. On the TV, open **Settings › Remotes and Devices › Remote App and Devices**.
2. In Lodestar's **Devices** page, wait for it to say **Ready to pair**, then
   choose **Pair**.
3. Type the code shown on the TV.
4. **Leave the pairing screen on the TV** before you install. The TV only accepts
   installs outside it.

Keep the Mac and the TV on the same Wi-Fi, and not on a guest network.

## 4. Install an app

1. Go to **Install**.
2. **App.** Drag the `.ipa` or `.tipa` into the window, or click the drop zone. Lodestar
   shows its name, version, minimum OS and any extensions.
3. **Device.** Click the device you want. If only one device can run the app,
   it's already selected.
4. **Signing** (optional):
   * *Display name* and *Bundle ID* change what the app is called and identified as.
   * **Remove device restrictions** lets apps limited to certain models install anywhere.
   * **Enable file sharing** shows the app's documents in Finder and the Files app.
   * **Remove app extensions** uses fewer App IDs, which matters on a free account.
5. Press **Install**. The panel at the bottom walks through *Connect, Provision,
   Sign, Install*, with the percentage beside it. The first run takes longer
   because it creates your signing certificate.

When it finishes you'll see *"… is installed on …"*.

### First launch on iPhone or iPad

Open **Settings › General › VPN & Device Management**, tap your Apple ID under
*Developer App*, and choose **Trust**. Then open the app. On an Apple TV, follow
any prompt the TV shows.

## 5. Keep it running: refresh

Signatures last 7 days. Open **My Apps** to see a countdown for each app. The
sidebar shows **N due** when something has two days or less left.

* **Refresh** re-signs one app and reinstalls it over the existing copy, which normally
  keeps the app's data. Back up anything important first.
* **Refresh all** does every app whose device is online.

The device must be connected (USB, or on the same network for a TV). Lodestar
keeps a copy of each IPA for exactly this, in
`~/Library/Application Support/Lodestar/library`.

## Troubleshooting

**No devices appear.** Unlock the phone, tap Trust, try another cable. For a TV,
check the network and that *Local Network* is allowed for Lodestar in System
Settings › Privacy & Security.

**"Maximum number of certificates."** Apple allows very few development
certificates per account. Lodestar frees a slot automatically; if it can't, the
**Activity** page lists your certificates. Revoke one in Xcode › Settings ›
Accounts › Manage Certificates, then try again. Revoking a certificate stops the
apps it signed from opening until you refresh them.

**"Not enough available App IDs."** You've used this week's 10. Tick **Remove app
extensions**, or wait up to 7 days.

**Stuck at "Getting your signing certificate".** macOS is waiting for you to allow
Keychain access, and the prompt can sit behind other windows. Find it, enter your
Mac password and choose **Always Allow**. This happens once per build of Lodestar, so
rebuilding from source brings it back (signing with the same identity avoids that).

**Sign-in fails or loops.** Check your password at appleid.apple.com. Lodestar only
saves a password after Apple accepts it, so a typo is never retried in the background.

**An app installs but closes immediately.** The app is probably built for another
platform or needs a newer OS than your device runs. The Install page shows its
minimum OS before you commit.

**Anything else.** Open **Activity** for the full log, or launch with
`LODESTAR_LOG=debug` from a terminal. Please include it, with your Apple ID
removed, in a bug report.

## Uninstalling

Delete `Lodestar.app`. To remove its data too, delete
`~/Library/Application Support/Lodestar` and the *Lodestar* and *isideload* items
in Keychain Access. Apps you installed stay on your devices until you delete them there.
