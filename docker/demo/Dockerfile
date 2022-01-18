FROM ubuntu:21.10
RUN apt-get update && apt-get install -y systemd
RUN apt-get update && apt-get install -y tree vim
RUN apt-get update && apt-get install -y -d nginx
RUN apt-get update && apt-get install -y -d php8.0-fpm
RUN apt-get update && apt-get install -y -d mariadb-server
CMD [ "/usr/lib/systemd/systemd", "--log-level=debug", "--unit=sysinit.target" ]