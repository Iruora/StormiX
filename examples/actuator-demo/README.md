# actuator-demo

A tiny Spring Boot 3.5 API with Actuator and Prometheus turned on, to try Stormix's
API CPU / memory / health monitor on your own machine.

## Run it

With Docker (nothing else needed):

    docker build -t actuator-demo .
    docker run --rm -p 8080:8080 actuator-demo

Or with Java 17+ and Maven:

    mvn package -DskipTests
    java -jar target/actuator-demo-1.0.jar

## URLs

| What | URL |
|---|---|
| Actuator base (Stormix `actuator` monitor) | http://localhost:8080/actuator |
| Prometheus text (Stormix `prometheus` monitor) | http://localhost:8080/actuator/prometheus |
| Health | http://localhost:8080/actuator/health |
| Cheap request | GET http://localhost:8080/api/hello |
| Burns CPU for ~20 ms | GET http://localhost:8080/api/work?ms=20 |
| Grows the heap by 256 KB (resets at ~512 MB) | GET http://localhost:8080/api/alloc?kb=256 |
| Make health DOWN / UP | POST http://localhost:8080/db/down , POST http://localhost:8080/db/up |

## Load test it

Open `examples/actuator-demo/actuator-demo.stormix.yaml` in Stormix and start the load test, or:

    stormix load actuator-demo.stormix.yaml

To see the "API went unhealthy" warning, run `curl -X POST localhost:8080/db/down`
while the test runs, then `curl -X POST localhost:8080/db/up`.
