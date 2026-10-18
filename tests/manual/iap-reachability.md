# IN-APP reachability and Unity compatibility

IN-APP emulates StoreKit locally. It also reports a Wi-Fi-style reachable route
through SCNetworkReachability, for Unity and other engines that test connectivity
before opening their purchase UI. This is not an internet connection: socket,
DNS and HTTP restrictions still follow the existing network-access option.
Server logins, receipt verification and HTTP-based connectivity tests are not
emulated. Do not treat the reachable hint as evidence that a server is available.

## Manual checks

- With IN-APP off and network access disabled, GetFlags and the initial run-loop
  callback must report zero (offline).
- With IN-APP on and network access disabled, both paths must report Reachable
  only: no WWAN, IsDirect, ConnectionRequired or InterventionRequired bits.
- With real network access enabled, both paths must report Reachable regardless
  of the IN-APP toggle.
- Toggle IN-APP and poll GetFlags again: the new result must reflect the toggle.
  Disabling IN-APP must not turn off separately enabled real network access.
- In a Unity game with a local StoreKit shop, enable IN-APP before opening the
  shop. Check whether a connectivity preflight now allows product loading and
  the purchase flow. Record the game/version and logs if it still says offline.
- Confirm actual HTTP/socket traffic is still blocked without network permission.

## Known limits

Reachability run-loop scheduling currently delivers an initial notification,
not ongoing network-change events. Dispatch-queue scheduling is not implemented.
Games that cache offline at launch or rely exclusively on change notifications
may not notice a mid-session toggle. For a startup test, launch the emulator with
TOUCHHLE_IAP_EMULATION=1 in its environment so emulation is on before the game's
first reachability query. The environment override stays on for that process;
unset it and relaunch to test disabling emulation.

If a game checks its own backend or a Unity HTTP endpoint rather than
SCNetworkReachability, this compatibility hint will not remove its offline UI.
