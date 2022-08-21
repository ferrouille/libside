# `libside`: a library for building configuration management tools

`libside` is a library that you can use to build a configuration management tool. It focuses on:

* Composability: multiple projects can be deployed on the same server
* Reversibility: it should be possible to undo any change the tool makes; if possible, no existing files should be overwritten
* Static verification: dependencies and requirements should be encoded with types when possible, so that it becomes harder to write an incorrect configuration

## Approach

`libside` is built around two main operations: build and apply. When building, the tool generates a dependency graph of requirements from the packages in `<root-dir>/packages`. The dependency graph is serialized to disk. When applying, a dependency graph of requirements is applied to the current system state.

Each tool needs to define its own configuration format for packages. This format can be concise, since it only needs to account for configuration you specifically need.

## Testing
If you want to run all tests, you need to install `lxc`. Some tests are run in `lxc` VMs.

## Warning notes
* This is unfinished software.
* You should probably assume this software contains bugs that can delete all your files. Backup your files before you run it.
* Run on trusted input only. Some input values can be abused for arbitrary code execution (sometimes intentionally).

# License
All code in this repository is licensed under the [AGPL-3.0 license](LICENSE.md).