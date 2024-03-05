#!/bin/sh

base_dir="/var/lib/rancher/rke2/etc/containerd"

mkdir -p "${base_dir}"
cp config.toml.tmpl "${base_dir}/config.toml.tmpl"
