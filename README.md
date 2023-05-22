# OTPTray

otptray is a Linux / macOS system tray TOTP generator. It provides a
convenient method to copy / paste your TOTP / 2FA codes to your
clipboard for authentication.

## Linux

On Linux, otptray runs as an appindicator application. You can copy
the current TOTP code to your clipboard by selecting the revelant
secret from the tray dropdown:

![Linux appindicator dropdown](https://raw.github.com/blakesmith/otptray/master/assets/linux_dropdown.png)

You can configure otptray and setup your TOTP secrets by selecting the
`Setup` menu item from the dropdown.

![Linux configuration screen](https://raw.github.com/blakesmith/otptray/master/assets/linux_configure.png)

You can also edit `$HOME/.config/otptray.yaml` to setup your TOTP
secrets manually:

```yaml
---
entries:
  - name: Google
    step: 30
    secret_hash: <TOTP Secret here>
    hash_fn: sha1
    digit_count: 6
  - name: GitHub
    step: 30
    secret_hash: <TOTP Secret here>
    hash_fn: sha1
    digit_count: 6
  - name: Facebook
    step: 30
    secret_hash: <TOTP Secret here>
    hash_fn: sha1
    digit_count: 6
```

## macOS

otptray also works on macoOS, though the configuration dialog is not
currently functional. You'll need to setup the YAML file manually, but
everything should work fine after that.

![macoOS system tray dropdown](https://raw.github.com/blakesmith/otptray/master/assets/macos_dropdown.png)

On macOS, the YAML file should be located at:

`$HOME/Library/Application\ Support/otptray.yaml`

## Building on Linux

If you use the nix package manager, from the root of this repo, with flakes enabled:

```
$ nix build
```

The `otptray` executable will be in `result/bin/otptray`.

## Building on macOS

TODO
