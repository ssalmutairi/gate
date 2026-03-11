package com.gate.health;

import com.gate.config.ProxyConfig;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.server.HandlerFunction;
import org.springframework.web.reactive.function.server.ServerRequest;
import org.springframework.web.reactive.function.server.ServerResponse;
import reactor.core.publisher.Mono;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

@Component
public class ServicesHandler implements HandlerFunction<ServerResponse> {

    private final ProxyConfig proxyConfig;

    public ServicesHandler(ProxyConfig proxyConfig) {
        this.proxyConfig = proxyConfig;
    }

    @Override
    public Mono<ServerResponse> handle(ServerRequest request) {
        List<Map<String, Object>> services = proxyConfig.getAllServices().values().stream()
                .map(s -> {
                    Map<String, Object> info = new LinkedHashMap<>();
                    info.put("name", s.name());
                    info.put("url", s.baseUrl());
                    info.put("timeout", s.timeoutSeconds());
                    info.put("auth", s.apiKey() != null);
                    if (s.host() != null) info.put("host", s.host());
                    return info;
                })
                .toList();

        return ServerResponse.ok().bodyValue(Map.of("services", services, "total", services.size()));
    }
}
