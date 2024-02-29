<div align="center">

# Sync: Neorg ⇄ GTask

Sync your Neorg TODO-items to Google tasks and vice-versa.

</div>

## What?

`neorg-task-sync` syncs all uncompleted todo-entries from your neorg journal to a google task list and vice versa.
It allows you to double-check/complete todos on your mobile phone while commuting.
Alternatively, this workflow allows for noting down todos while away from your computer and still have them appear in the newest neorg journal file.

By default, new tasks are synced to the latest journal entry .
Tasks are synced to the last (optionally first) file you specify on the command line, optionally to a specific section (by default `TODO`).
This ensures that the latest journal file receives new remote tasks.

While syncing the your journal is the intended use-case it also allows syncing to any other file. 

If configured, 

## Why?

I keep notes via [Neorg's journal feature](https://github.com/nvim-neorg/neorg/wiki/Journal).
However, I would also like to add TODOs when I am away from the PC (for example after spontaneous meetings).
Furthermore, I like to go through and update older TODOs while commuting.
For obvious reasons, Neorg's journaling feature cannot solve this, but Google Tasks does.

Hence, the _obvious_ solution was to write a quick syncer.
The required metadata is concealed via treesitter-rules.

## How? (Installation)

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

## Configuraiton

TODO

# Command-Line Help for `neorg-task-sync`

This document contains the help content for the `neorg-task-sync` command-line program.

**Command Overview:**

* [`neorg-task-sync`↴](#neorg-task-sync)
* [`neorg-task-sync auth`↴](#neorg-task-sync-auth)
* [`neorg-task-sync auth login`↴](#neorg-task-sync-auth-login)
* [`neorg-task-sync config`↴](#neorg-task-sync-config)
* [`neorg-task-sync config import`↴](#neorg-task-sync-config-import)
* [`neorg-task-sync config show`↴](#neorg-task-sync-config-show)
* [`neorg-task-sync config tasklist`↴](#neorg-task-sync-config-tasklist)
* [`neorg-task-sync generate`↴](#neorg-task-sync-generate)
* [`neorg-task-sync generate help-markdown`↴](#neorg-task-sync-generate-help-markdown)
* [`neorg-task-sync generate completion`↴](#neorg-task-sync-generate-completion)
* [`neorg-task-sync parse`↴](#neorg-task-sync-parse)
* [`neorg-task-sync sync`↴](#neorg-task-sync-sync)
* [`neorg-task-sync tasks`↴](#neorg-task-sync-tasks)

## `neorg-task-sync`



**Usage:** `neorg-task-sync [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `auth` — Auth related commands
* `config` — Show config
* `generate` — Generate completions
* `parse` — Run a parse action (mainly for debugging)
* `sync` — Sync tasks between local file and google tasks
* `tasks` — Check which tasks are defined upstream (mainly for debugging)

###### **Options:**

* `-v`, `--verbose` — Make output more verbose



## `neorg-task-sync auth`

Auth related commands

**Usage:** `neorg-task-sync auth <COMMAND>`

###### **Subcommands:**

* `login` — 



## `neorg-task-sync auth login`

**Usage:** `neorg-task-sync auth login`



## `neorg-task-sync config`

Show config

**Usage:** `neorg-task-sync config <COMMAND>`

###### **Subcommands:**

* `import` — 
* `show` — 
* `tasklist` — 



## `neorg-task-sync config import`

**Usage:** `neorg-task-sync config import [OPTIONS] <WHAT>`

###### **Arguments:**

* `<WHAT>` — what to import

  Possible values: `client-secret`


###### **Options:**

* `-f`, `--file <FILE>` — Read from file



## `neorg-task-sync config show`

**Usage:** `neorg-task-sync config show`



## `neorg-task-sync config tasklist`

**Usage:** `neorg-task-sync config tasklist <OPERATION> [VALUE]`

###### **Arguments:**

* `<OPERATION>`

  Possible values:
  - `get`:
    Get current value
  - `set`:
    Set current value
  - `list`:
    List possible values current value

* `<VALUE>` — Value (for set operation)



## `neorg-task-sync generate`

Generate completions

**Usage:** `neorg-task-sync generate <COMMAND>`

###### **Subcommands:**

* `help-markdown` — Generate markdown from help messages
* `completion` — Copmletion script



## `neorg-task-sync generate help-markdown`

Generate markdown from help messages

**Usage:** `neorg-task-sync generate help-markdown`



## `neorg-task-sync generate completion`

Copmletion script

**Usage:** `neorg-task-sync generate completion <SHELL>`

###### **Arguments:**

* `<SHELL>` — Shell to generate completions for

  Possible values: `bash`, `elvish`, `fish`, `powershell`, `zsh`




## `neorg-task-sync parse`

Run a parse action (mainly for debugging)

**Usage:** `neorg-task-sync parse [OPTIONS] <TARGET>`

###### **Arguments:**

* `<TARGET>` — What to generate

###### **Options:**

* `-f`, `--force-norg` — Force parsing even if extension does not match



## `neorg-task-sync sync`

Sync tasks between local file and google tasks

**Usage:** `neorg-task-sync sync [OPTIONS] <FILES_OR_FOLDERS>...`

###### **Arguments:**

* `<FILES_OR_FOLDERS>` — Files or folders to sync. New remote tasks will be synced into the last file specified (after sorting)

###### **Options:**

* `--fix-missing`
* `-f`, `--pull-to-first` — Pull new remote tasks to first file specified, instead
* `-s`, `--without-sort` — Do not sort filenames prior to syncing
* `-L`, `--without-local` — Do not sync remote google tasks to local todos (neither create nor update status)
* `-R`, `--without-remote` — Do not sync local todos to remote google tasks (neither create nor update status)
* `-r`, `--without-push` — Do not push local todos to google and create new tasks
* `-l`, `--without-pull` — Do not pull remote google tasks and insert them into the todo section



## `neorg-task-sync tasks`

Check which tasks are defined upstream (mainly for debugging)

**Usage:** `neorg-task-sync tasks [OPTIONS]`

###### **Options:**

* `-j`, `--json` — output as json



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
