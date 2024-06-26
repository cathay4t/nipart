# Title: ADR-006: Persist Configuration and Manage Checkpoint

## Status: Proposed

## Context

User needs persistent configuration and checkpoint.

Use cases for persisting configurations:
 * Tracking changes between apply requests.
 * Converting changes made by external tools to persistent.

Use cases for managing checkpoint:
 * Multiple con-current checkpoints tracking
 * Checkpoint rollback or commit
 * Provide incremental network change report between checkpoint point-of-time
   network state and current.

## Decision

Introducing `NipartRole::Tracking` plugin hooking on these events:

 * Network state changes desired by user or plugins.
 * Network changes made by external tools.
 * Configurations changes in `/etc/nipart` folder.

In order to avoid tracking intermediate states:

 * During `ApplyNetState`, all monitor rules will be suspended.

 * When external events happens, we wait some time till last notification
   arrived. For example:
    1. Nipart only apply state in /etc folder after modification been made in
       10(number might change during implementation) seconds.
    2. Nipart only trace network state changes after 10 seconds(not decided
       yet) truce since last netlink message arrived.

 * Place prohibitor on bouncing changes.

The `NipartRole::Tracking` plugin will tracking two types of network changes:

 * Non-volatile
   Network changes made by user through nipart. Including explicit applying
   request or converting volatile state. This state will be persistent after
   reboot. Will use `/var/lib/nipart` folder to do non-volatile state tracking.
   The `/etc/nipart` folder is for user configuration, no git tracking there.

 * Volatile
   Network changes will be not restored by nipart after reboot. Including
   but not limited to changes make external tools, DHCP lease, link status.
   Will use `/run/nipart` folder for volatile state tracking.

The `NipartRole::Tracking` plugin should provide a way to disable state
tracking and only use /etc/nipart for one-time(up on start of daemon) network
configuration.

### Approaches been considered but dismissed:

#### A: Providing API supporting a subset of git commands.

Reason of dismission: User have to learn new terminologies and niport have to
maintain its own git-mimicking code. It means we are reinvent the wheel of git.

#### B: Old fashion way of using files in /etc and /run

Reason of dismission: Very complex to support git-link manner for history
tracking and multiple con-concurrent checkpoint tracking using file based
backend in /etc and /run folder. It means we are reinvent the wheel of git.

## Consequences

### Better

 * Using existing git workflow will smooth the learning curve of adopting
   nipart configuration processing and checkpoint maintaining.

 * Easy to switch between network stats made by user/AI/external.

### Worse

#### Not friendly to user without git experience

User can still place configurations to /etc/nipart folder or made changes via
`nipc apply`, `nipc history-show` or `nipc revert` without any knowledge of how
nipart process the configurations.

#### The `git show <commit_id>` is hard to understand without interface name

Nipart should provide helper commands using result of `Nmstate::gen_diff()`
for better interpretation of network changes. The direct use of `git` against
`/var/lib/nipart` folder is for advanced user.
