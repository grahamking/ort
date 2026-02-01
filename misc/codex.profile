# firejail profile for OpenAI's Codex
# Copy it to ~/.config/firejail/codex.profile
#
# Usage: firejail codex --search --model gpt-5.2-codex
# If the profile isn't called exactly 'codex.profile', pass `--profile=<name>`
#
# Set a bash alias so you don't forget:
# alias codex='firejail /home/username/bin/codex --search --model gpt-5.2-codex'

### Basic Blacklisting ###
include disable-common.inc	# dangerous directories like ~/.ssh and ~/.gnupg
#include disable-exec.inc	# non-executable directories such as /var, /tmp, and /home
#include disable-programs.inc	# user configuration for programs such as firefox, vlc etc.
#include disable-shell.inc	# sh, bash, zsh etc.
#include disable-xdg.inc	# standard user directories: Documents, Pictures, Videos, Music

### Home Directory Whitelisting ###

read-only ${HOME}
read-write ${HOME}/src
read-write ${HOME}/.cargo
read-write ${HOME}/.rustup
read-write ${HOME}/.codex

#apparmor	# if you have AppArmor running, try this one!
caps.drop all
ipc-namespace
netfilter
#no3d	# disable 3D acceleration
#nodvd	# disable DVD and CD devices
#nogroups	# disable supplementary user groups
#noinput	# disable input devices
nonewprivs
noroot
#notv	# disable DVB TV devices
#nou2f	# disable U2F devices
#novideo	# disable video capture devices
protocol unix,inet,inet6,
#net eth0
netfilter
seccomp !chroot	# allowing chroot, just in case this is an Electron app
#tracelog	# send blacklist violations to syslog
allow-debuggers
memory-deny-write-execute

disable-mnt	# no access to /mnt, /media, /run/mount and /run/media
private-dev
private-tmp

