# DIY: Disable / Re-enable Zscaler Manually

All commands require root. Replace `<uid>` with your numeric user ID (`id -u`).

---

## Disable ("muzzle off")

```sh
sudo launchctl disable system/com.zscaler.tunnel
sudo launchctl bootout  system/com.zscaler.tunnel

sudo launchctl disable system/com.zscaler.service
sudo launchctl bootout  system/com.zscaler.service

sudo launchctl disable system/com.zscaler.UPMServiceController
sudo launchctl bootout  system/com.zscaler.UPMServiceController

sudo launchctl disable gui/<uid>/com.zscaler.tray
sudo launchctl bootout  gui/<uid>/com.zscaler.tray
```

`bootout` exits non-zero if the service was already stopped — that is harmless.

---

## Re-enable ("muzzle on")

```sh
sudo launchctl enable    system/com.zscaler.tunnel
sudo launchctl bootstrap system /Library/LaunchDaemons/com.zscaler.tunnel.plist

sudo launchctl enable    system/com.zscaler.service
sudo launchctl bootstrap system /Library/LaunchDaemons/com.zscaler.service.plist

sudo launchctl enable    system/com.zscaler.UPMServiceController
sudo launchctl bootstrap system /Library/LaunchDaemons/com.zscaler.UPMServiceController.plist

sudo launchctl enable    gui/<uid>/com.zscaler.tray
sudo launchctl bootstrap gui/<uid> /Library/LaunchAgents/com.zscaler.tray.plist
```

`bootstrap` exits non-zero if the service was already loaded — that is harmless.
