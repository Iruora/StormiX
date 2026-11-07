package demo;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;
import org.springframework.boot.SpringApplication;
import org.springframework.boot.actuate.health.Health;
import org.springframework.boot.actuate.health.HealthIndicator;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.context.annotation.Bean;
import org.springframework.web.bind.annotation.*;

/** A tiny API to load test with Stormix while it watches /actuator. */
@SpringBootApplication
@RestController
public class DemoApp {
    private final AtomicBoolean dbUp = new AtomicBoolean(true);
    private final List<byte[]> kept = new ArrayList<>();

    public static void main(String[] args) { SpringApplication.run(DemoApp.class, args); }

    /** Cheap request. */
    @GetMapping("/api/hello")
    public Map<String, Object> hello() { return Map.of("hello", "stormix"); }

    /** Burns CPU: ?ms=20 spins for about that long. */
    @GetMapping("/api/work")
    public Map<String, Object> work(@RequestParam(defaultValue = "20") long ms) {
        long end = System.nanoTime() + ms * 1_000_000, n = 0;
        while (System.nanoTime() < end) n += Math.sqrt(n + 1) > 0 ? 1 : 0;
        return Map.of("spins", n);
    }

    /** Grows the heap: keeps ?kb=256 per call, up to about 512 MB. */
    @GetMapping("/api/alloc")
    public synchronized Map<String, Object> alloc(@RequestParam(defaultValue = "256") int kb) {
        if (kept.size() * (long) kb > 512 * 1024) kept.clear();
        kept.add(new byte[kb * 1024]);
        return Map.of("blocks", kept.size());
    }

    /** POST /db/down and /db/up make /actuator/health report DOWN or UP. */
    @PostMapping("/db/{state}")
    public Map<String, Object> db(@PathVariable String state) {
        dbUp.set(!"down".equals(state));
        return Map.of("db", dbUp.get() ? "UP" : "DOWN");
    }

    @Bean
    HealthIndicator db() {
        return () -> dbUp.get() ? Health.up().build() : Health.down().withDetail("reason", "turned off with POST /db/down").build();
    }
}
