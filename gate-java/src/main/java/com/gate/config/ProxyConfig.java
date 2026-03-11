package com.gate.config;

import com.fasterxml.jackson.annotation.JsonProperty;
import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;
import jakarta.annotation.PostConstruct;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.context.annotation.Configuration;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

@Configuration
public class ProxyConfig {

    private static final Logger log = LoggerFactory.getLogger(ProxyConfig.class);

    private final Map<String, ServiceEntry> services = new ConcurrentHashMap<>();

    public record ServiceEntry(
            String name,
            String ip,
            int port,
            @JsonProperty("api-key") String apiKey,
            @JsonProperty("tls") Boolean tls,
            @JsonProperty("timeout") Integer timeout,
            @JsonProperty("host") String host
    ) {
        public String baseUrl() {
            String scheme = Boolean.TRUE.equals(tls) ? "https" : "http";
            return scheme + "://" + ip + ":" + port;
        }

        public int timeoutSeconds() {
            return timeout != null && timeout > 0 ? timeout : 30;
        }
    }

    @PostConstruct
    public void init() {
        String proxy = System.getenv("PROXY");
        if (proxy == null || proxy.isBlank()) {
            log.warn("PROXY env var is not set — no services configured");
            return;
        }

        try {
            List<ServiceEntry> entries = new ObjectMapper()
                    .readValue(proxy, new TypeReference<>() {});

            for (ServiceEntry entry : entries) {
                services.put(entry.name(), entry);
                log.info("Service registered: {} -> {}:{} (api-key: {})",
                        entry.name(), entry.ip(), entry.port(),
                        entry.apiKey() != null ? "***" : "none");
            }
            log.info("Loaded {} services from PROXY", services.size());
        } catch (Exception e) {
            throw new IllegalStateException("Failed to parse PROXY: " + e.getMessage(), e);
        }
    }

    public ServiceEntry getService(String name) {
        return services.get(name);
    }

    public Map<String, ServiceEntry> getAllServices() {
        return Collections.unmodifiableMap(services);
    }
}
