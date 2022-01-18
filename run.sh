# /bin/bash

# Using podman as a lightweight VM alternative, because starting a VM from a fresh state every time is a pain.
cargo build --example demo --release || exit
podman build docker/demo -t side-demo
PWD=`pwd`
ID=`podman run -d --cap-add CAP_DAC_READ_SEARCH --cap-add CAP_FOWNER --cap-add CAP_SYS_ADMIN --cap-add CAP_SYS_CHROOT --cap-add CAP_SYS_RESOURCE --cap-add CAP_SYS_RAWIO --cap-add CAP_AUDIT_CONTROL --cap-add CAP_AUDIT_READ --cap-add CAP_AUDIT_WRITE --cap-add CAP_CHOWN --cap-add CAP_DAC_OVERRIDE --cap-add CAP_DAC_READ_SEARCH --cap-add CAP_MKNOD --cap-add SYS_PTRACE --mount "type=bind,src=$PWD/target/release/examples/,target=/target" --mount "type=bind,src=$PWD/demo-data,target=/data" -p 8017:80 -p 1229:1229 side-demo`
echo Running as $ID

cleanup() {
    echo Stopping...
    podman container stop $ID

    echo Deleting...
    podman container rm $ID
}

trap cleanup EXIT

sleep 1
podman exec $ID /data/add-convenience-commands.sh
podman exec $ID /target/demo /srv init && \
echo && echo && echo && \
podman exec $ID cp -r /data/packages/helloworld.test /srv/packages && \
echo && echo && echo && \
podman exec $ID /target/demo /srv build && \
echo && echo && echo && \
podman exec $ID cp -r /data/packages /srv && \
echo && echo && echo && \
podman exec $ID /target/demo /srv build && \
echo && echo && echo && \
podman exec $ID /target/demo /srv build

podman exec -it $ID bash
CODE=$?
echo "Return code: $CODE"

if [ $CODE -eq 7 ]; then
    cleanup
    trap - EXIT
    ./run.sh
fi