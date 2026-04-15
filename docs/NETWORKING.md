# Chatty-EDU Local Networking

This document explains Chatty-EDU's optional local networking feature in plain language.

## What it is

Chatty-EDU can optionally connect to other **nearby Chatty-EDU instances on the same local Wi-Fi or LAN**.

This is for:
- handing work off between nearby devices
- sharing lightweight status like which tab is open
- helping one local EDU instance tell another what it should pick up next
- sending classroom setup bundles so one EDU device can mirror lesson-ready settings to another
- sending homework packs and revision packs through their own inbox lanes
- making nearby classroom devices easier to recognize with local names and group labels

This is **not** a cloud feature.

## What it is not

Chatty-EDU networking is **not**:
- internet syncing
- account-based multi-user sign-in
- cloud storage
- remote telemetry
- hidden background sharing

If you never turn it on, Chatty-EDU still works normally.

## Why it exists

The goal is to let schools, teachers, or local teams use more than one Chatty-EDU machine together **without breaking the local-first design**.

Examples:
- one teacher laptop hosts and another nearby machine connects
- one support machine hands off a short note to another local machine
- one EDU instance passes a brief update to another in the same room or local network
- one teacher pushes the current homework pack to the active classroom devices
- one teacher pushes the latest revision pack or a classroom setup bundle before a lesson starts

It is meant to support local teamwork without sending data to outside services.

## How it works

From the user side:
1. Open the `Network` menu or the `Networking` tab.
2. On one machine, turn on `Make available for connectivity`.
3. On another machine, click `Refresh discovery`.
4. When a device appears, click `Connect`.
5. Use the handoff panel to send a short note if needed.

For classroom workflows, the common pattern is:
1. connect the classroom devices
2. prepare the lesson on the teacher machine
3. use `Push Pack`, `Push Revision`, or `Push Setup`
4. let the receiving device preview the item in its inbox
5. apply it only when you are ready

What gets shared:
- device name
- active tab label
- short local status text
- selected model label
- optional short handoff messages

What can also be sent deliberately:
- **homework packs** -> received into a homework inbox first
- **revision packs** -> received into a revision inbox first
- **classroom setup bundles** -> received into a setup-bundle inbox first
- **module shared state** -> delivered through the module bridge when a module publishes it
- **session events** -> lightweight classroom/module signals for room-aware or multiplayer modules
- **chunked file-style transfers** -> the local transport can now carry larger text or binary payloads for future classroom or module features

That separation matters:
- homework content stays in the homework lane
- revision content stays in the revision lane
- setup bundles only carry lesson or app setup preferences
- modules keep owning their own state and only share what their optional bridge plug exposes

Nothing here is meant to be hidden or silently auto-applied.

## Managing several classroom devices

When you only have one or two nearby devices, the default names are often enough.

When you have several teacher, support, or classroom devices visible at once, the networking tab is easier to use if you:
- rename devices to something human-readable
- add short class or group labels
- use the search box to narrow the list quickly

Examples:
- `Front Row Laptop`
- `Teacher Desk`
- `Support Tablet`
- group labels like `Maths A`, `Reading Circle`, or `Science Lab`

That is why Chatty-EDU now lets you:
- click a device name to set a custom local alias
- click the group chip to set or clear a class/group label
- click `Trust` to remember a classroom device by its stable device ID
- search by name, device ID, address, or group label
- use `Select Connected` when you want to act on the active classroom set quickly
- use `Copy ID` or `Copy info` when you need to confirm exactly which device is which

Important note:
- aliases and group labels are **local preferences on your machine**
- they exist to make your device list easier to manage
- they do **not** create school accounts, cloud identities, or hidden tracking
- Chatty-EDU now keeps a **stable local device ID** across restarts, so teacher aliases, class/group labels, blocked-device rules, and classroom-room roles stay attached to the same device

## Allow vs Trust vs Block

When `Allow unknown devices` is turned off, new classroom devices ask first.

You now have three clear choices:
- **Allow** -> approve this device for the current running session
- **Trust** -> remember this device's stable device ID so future classroom joins are approved automatically
- **Block** -> remember a deny rule until a teacher unblocks that device

That gives you a calmer classroom control model:
- `Allow` for one-off joins
- `Trust` for devices that belong in the room regularly
- `Block` for devices that should stay out until you deliberately reverse it

Trusted devices appear in their own section in the Networking tab, so teachers can review and remove remembered pairings without treating those devices as blocked.

You can also now:
- click `Export trusted list` to save the current classroom trust list as a portable JSON file
- click `Import trusted list` on another teacher machine to reuse that remembered classroom pairing set
- click `Export blocked list` to carry current deny rules to another teacher machine
- click `Import blocked list` when that machine should inherit the same blocked-device policy

That is useful when more than one teacher laptop or support machine needs the same trusted-room baseline.

## Host handoff and session recovery

Classroom-room and module-room sessions now keep a lightweight **recoverable host snapshot** on the current host machine.

- If the teacher/host device restarts, open Networking and use `Resume saved session`.
- If another teacher or support device should take over cleanly, select that connected device and click `Hand off host to selected peer`.
- If the host disappears unexpectedly, the remaining devices will see `Current room host appears offline` and can choose `Take over as host`.

This is intentionally explicit:
- classroom control does not silently jump between machines
- recovery only arms on the current host
- handoff keeps the same room/session identity while moving authority cleanly

Recovery now also keeps the **latest module session bridge state** alongside room ownership:
- `Restore state to bridge` rewrites the last cached `shared_state.json` back into the hosted lesson module bridge after a restart
- `Re-share latest state` pushes that last known good lesson/module state back out to selected devices, or to the room if nothing is selected
- `Replay cached assets` is the companion lane for future module-tagged lesson files/assets that belong to that same host-owned session

That means recovery is no longer just about who hosts the room. It also helps the teacher machine rehydrate the module's last good session state before the lesson carries on.

## Everyday classroom workflow

For a practical local classroom setup:
1. Turn on `Make available for connectivity` on the device that should be visible.
2. Click `Refresh discovery` on the other machine.
3. Rename repeated/default-looking devices so they stay easy to recognize.
4. Add group labels if that helps you sort by class, table, or role.
5. Use `Select Connected` when you want to act on the currently active set quickly.
6. Use `Copy info` if you need to confirm a specific device before taking action.
7. If a device belongs in the room regularly, click `Trust` so the teacher machine remembers it cleanly across restarts.
8. Stable device IDs now survive restarts, so classroom naming and blocking decisions stay anchored to the same devices.
9. If another teacher machine should inherit the same remembered room devices, export the trusted list here and import it there.
10. If that machine should inherit the same deny rules too, export the blocked list here and import it there.

This is especially useful when several nearby EDU devices are online at the same time.

## The three transfer lanes

Chatty-EDU now uses three different transfer lanes on purpose:

- **Homework pack lane**  
  For actual homework content. These arrive in the homework inbox and can then be applied into `homework/assigned/`.

- **Revision pack lane**  
  For revision markdown packs. These arrive in the revision inbox and can then be applied into the revision workspace.

- **Classroom setup bundle lane**  
  For lesson-wide setup, such as teacher mode, default year level, Janet/game/voice toggles, and model hints. These arrive in their own inbox and can then be applied to the local EDU instance.

- **Session-event lane**  
  For fast classroom/module session signals such as ready states, turn nudges, tiny game moves, or other small lesson-room updates. These are mirrored into `bridge/shared_room_events.json` for hosted room-aware modules instead of going through the heavier inbox/apply flow.

Why we keep them separate:
- it keeps classroom content and app setup from getting mixed together
- it makes preview and approval clearer
- it keeps the bundle flow lightweight
- it avoids over-sharing when only a small setup change is needed

Important boundary:
- setup bundles do **not** carry teacher PINs, secret answers, blocked-device lists, or student identity data
- homework and revision content keep using their own purpose-built pack formats

What does **not** happen automatically:
- no internet upload
- no cloud account sync
- no automatic always-on sharing

## Transfer support and limits

The local transport layer is now bigger than a tiny note pipe.

Current practical support:
- plain text transfers
- JSON and Markdown transfers
- chunked larger text payloads
- binary/file-style payloads for future classroom tools and modules
- lightweight room/session event messages for fast module-state nudges

Current limits in this build generation:
- maximum decoded payload size: **8 MiB**
- chunk size: **64 KiB** per packet
- retry window: **up to 3 send attempts** waiting for a final delivery acknowledgement

In everyday terms:
- homework, revision, setup-bundle, luke-warm, and shared-state lanes still behave the same from the user side
- larger local packs do not need to fit into one tiny packet anymore
- binary payloads are supported by the transport, even if today's built-in inbox screens mostly show metadata unless a specific EDU feature or module knows how to use the file

What this is **not** trying to be:
- a public internet file sync system
- a school cloud storage service
- an unbounded asset streaming platform

The target is simply solid local-room transfers that a future EDU module builder would reasonably expect.

## Important boundaries

- Networking is **local only**.
- It is **off by default** until a user enables availability.
- It is meant for **nearby EDU peers**, not public internet access.
- Chatty-EDU and Chatty-Cog use different local networking identifiers, so they do **not** accidentally connect to each other.

## Offline promise, clarified

Chatty-EDU is still best described as:

> local-first, no-cloud, no-calls-home

That promise still holds because:
- the app does not require internet to function
- it ships with no cloud endpoints
- local networking is optional and user-triggered
- data stays inside the local environment unless a user deliberately moves files or enables local peer connectivity
- received packs and setup bundles land in inboxes first, so a person still chooses when to apply them

## Security and deployment notes

For school IT:
- discovery uses local LAN broadcast
- the discovery port is `45841`
- hosted peer sessions use a dynamically chosen local TCP port
- local firewall rules may need to allow Chatty-EDU on trusted local networks
- received transfer inboxes live under `network_inbox/`
  - `network_inbox/homework_packs/`
  - `network_inbox/revision_packs/`
  - `network_inbox/workflow_bundles/`

If nearby EDU peers still do not connect cleanly:
- check the `Compatibility note` line in the Networking tab
- make sure both machines are on reasonably matching Chatty-EDU builds
- older local builds from before the chunked-transfer upgrade will show up as incompatible until they are rebuilt or updated
- remember that Chatty-EDU and Chatty-Cog intentionally use different local protocols and will not interconnect

This feature is a convenience layer, not a hardened trust boundary. Schools should still rely on:
- normal firewall policy
- device management
- OS accounts and permissions
- standard endpoint monitoring

## When to leave it off

Leave networking off if:
- the machine should stay fully self-contained
- the school does not want local peer discovery
- the deployment is for a single-device student setup
- local firewall policy should stay as strict as possible

## Good mental model

The simplest way to think about it is:

- **normal Chatty-EDU** = fully local on one machine
- **network-enabled Chatty-EDU** = still local, but able to talk to nearby EDU peers on the same LAN when you choose to allow it
- **renamed / grouped peers** = still the same local devices, just easier for you to recognize and manage
