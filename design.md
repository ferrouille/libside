## Directory structure:
* `/srv`
    * `packages`: The input for SiDE
        * `<package>`
            * Files needed for the package
            * `package.toml`: installation instructions for the package
    * `installed`: All files that your application needs, and that aren't user-generated
        * `<N>`
            * `db`: Databases of what exactly SiDE has modified on the rest of your server
                * `user`
                * `group`
                * `apt`
                * `file`
            * `generated`: Generated files that are referenced in the databases, and that will be copied over existing files (for example to `/srv/files/config`)
                * `<package>`
                    * Package (configuration) files generated when installing, for example nginx site configurations
    * `chroots`
        * `<N>`
            * `<package>`
                * chroot directories for the package
    * `files`
        * `exposed`
            * `<package>`
                * `<N>`
                    * package files that are in use, copied from /srv/packages
        * `config`
            * `<package>`
                * configuration files 
    * `data`
        * `<package>`
            * `userdata`
                * user data
            * `secrets`
                * secrets
    * `backups`
        * backup data

The package names `_start` and `_finish` are reserved for files and configuration that is needed for multiple packages.