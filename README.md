<div align="center">

# Sync: Neorg ⇄ GTask

Sync your Neorg TODO-items to Google tasks and vice-versa.

</div>

## What & why?

I keep notes via [Neorg's journal feature](https://github.com/nvim-neorg/neorg/wiki/Journal).
However, I would also like to add TODOs when I am away from the PC (for example after spontaneous meetings).
Futhermore, I like to go through and update older TODOs while commuting.
For obvious reasons, Neorg's journaling feature cannot solve this, but Google Tasks does.

Hence, the _obvious_ solution was to write a quick syncer.
_TODO: Expand…_


## Installation

Unfortunately, Google does not permit shipping client IDs with open source code.
Hence, the installation procedure is:
1. Install locally
2. Create your own Google project & OAuth token

### Install locally
1. Install as `neovim` plugin:
```vim
Plug 'obreitwi/neorg-task-sync' " vim-plug
```
2. Navigate to the plugin folder (or clone again to install):
```bash
git clone https://github.com/obreitwi/neorg-task-sync.git
cd neorg-task-sync
cargo install --path "$PWD"
# pre-create config folder
mkdir -p ~/.config/neorg-task-sync
```

### Create your own Google project & OAuth token 
0. (Register for [Google Cloud](https://console.cloud.google.com), if not done already…)
1. [Create a new Google Project](https://console.cloud.google.com/projectcreate).
   The name does not matter much since you will only use it for yourself.
   Alternatively, you can also re-use an existing private project of yours.
2. For your project, enable [Google Tasks API](https://console.cloud.google.com/marketplace/product/google/tasks.googleapis.com)
3. Create a [new OAuth 2.0 client](https://console.cloud.google.com/apis/credentials/oauthclient).
   For _Application Type_, choose "Desktop App" and any name.
4. After creation, make sure to download the OAuth client to
```
~/.config/neorg-task-sync/clientsecret.json
```
