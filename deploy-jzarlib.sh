cargo build --release -p czarlib
mvn -f jzarlib-ffm/pom.xml deploy

sh jzarlib-jni/build-native.sh
mvn -f jzarlib-jni/pom.xml deploy
