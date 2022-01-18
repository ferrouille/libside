#! /bin/bash

# An alias for the side demo binary
echo "alias side='/target/demo /srv'" >> ~/.bashrc

# Copy the modified packages into /srv and re-run side
echo "alias u='cp -r /data/packages /srv && /target/demo /srv build'" >> ~/.bashrc

# Quickly exit the container
echo "alias q='exit'" >> ~/.bashrc

# Rebuild & start a new instance
echo "alias n='exit 7'" >> ~/.bashrc