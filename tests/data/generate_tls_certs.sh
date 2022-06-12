#!/usr/bin/env bash
openssl req -subj '/CN=localhost' -x509 -newkey rsa:4096 -keyout key_pkcs8.pem -out cert.pem -nodes -days 3650
openssl rsa -in key_pkcs8.pem -out key_pkcs1.pem
