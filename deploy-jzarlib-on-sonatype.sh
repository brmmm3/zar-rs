#!/bin/sh
set -eu

export MAVEN_OPTS="${MAVEN_OPTS:-} -Dgpg.keyname=DD79135E26B949A4F60E9CF0C5A25902F4B38B7E"

cargo build --release -p czarlib
mvn -f jzarlib-ffm/pom.xml -Pcentral deploy

sh jzarlib-jni/build-native.sh --skip-czarlib-build
mvn -f jzarlib-jni/pom.xml -Pcentral deploy