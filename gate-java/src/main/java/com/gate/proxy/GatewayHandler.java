package com.gate.proxy;

import com.gate.config.ProxyConfig;
import com.gate.config.ProxyConfig.ServiceEntry;
import io.netty.channel.ChannelOption;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.client.reactive.ReactorClientHttpConnector;
import org.springframework.http.server.reactive.ServerHttpRequest;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;
import org.springframework.web.reactive.function.server.HandlerFunction;
import org.springframework.web.reactive.function.server.ServerRequest;
import org.springframework.web.reactive.function.server.ServerResponse;
import reactor.core.publisher.Mono;
import reactor.netty.http.client.HttpClient;

import java.net.URI;
import java.security.MessageDigest;
import java.time.Duration;
import java.util.Set;

@Component
public class GatewayHandler implements HandlerFunction<ServerResponse> {

    private static final Logger log = LoggerFactory.getLogger(GatewayHandler.class);
    private static final Set<String> SKIP_REQUEST_HEADERS = Set.of(
            "host", "accept-encoding", "connection", "transfer-encoding", "x-api-key");
    private static final Set<String> SKIP_RESPONSE_HEADERS = Set.of(
            HttpHeaders.TRANSFER_ENCODING.toLowerCase(), HttpHeaders.CONTENT_LENGTH.toLowerCase());

    private final ProxyConfig proxyConfig;
    private final WebClient webClient;

    public GatewayHandler(ProxyConfig proxyConfig) {
        this.proxyConfig = proxyConfig;

        HttpClient httpClient = HttpClient.create()
                .option(ChannelOption.CONNECT_TIMEOUT_MILLIS, 5_000);

        this.webClient = WebClient.builder()
                .clientConnector(new ReactorClientHttpConnector(httpClient))
                .codecs(c -> c.defaultCodecs().maxInMemorySize(16 * 1024 * 1024))
                .build();
    }

    @Override
    public Mono<ServerResponse> handle(ServerRequest request) {
        long start = System.nanoTime();

        String path = request.path();
        if (path.startsWith("/")) path = path.substring(1);

        int slash = path.indexOf('/');
        String serviceName = slash > 0 ? path.substring(0, slash) : path;
        String remaining = slash > 0 ? path.substring(slash) : "/";

        ServiceEntry service = proxyConfig.getService(serviceName);
        if (service == null) {
            log.warn("{} {} -> 404 (unknown service: {})", request.method(), request.path(), serviceName);
            return ServerResponse.status(HttpStatus.NOT_FOUND)
                    .bodyValue(new ErrorBody("Service not found: " + serviceName));
        }

        String query = request.uri().getRawQuery();
        String targetUrl = service.baseUrl() + remaining + (query != null ? "?" + query : "");

        // If service requires an API key, validate it from the incoming request
        if (service.apiKey() != null) {
            String clientKey = request.headers().firstHeader("X-API-KEY");
            if (clientKey == null || !MessageDigest.isEqual(
                    clientKey.getBytes(), service.apiKey().getBytes())) {
                log.warn("{} {} -> 401 (invalid or missing X-API-KEY for service: {})",
                        request.method(), request.path(), serviceName);
                return ServerResponse.status(HttpStatus.UNAUTHORIZED)
                        .bodyValue(new ErrorBody("Unauthorized: invalid or missing X-API-KEY"));
            }
        }

        ServerHttpRequest original = request.exchange().getRequest();

        HttpHeaders headers = new HttpHeaders();
        original.getHeaders().forEach((name, values) -> {
            if (!SKIP_REQUEST_HEADERS.contains(name.toLowerCase())) {
                headers.put(name, values);
            }
        });
        if (service.host() != null) {
            headers.set(HttpHeaders.HOST, service.host());
        }

        return request.bodyToMono(byte[].class)
                .defaultIfEmpty(new byte[0])
                .flatMap(bodyBytes -> {
                    var reqSpec = webClient
                            .method(request.method())
                            .uri(URI.create(targetUrl))
                            .headers(h -> h.addAll(headers));
                    var responseSpec = bodyBytes.length > 0
                            ? reqSpec.bodyValue(bodyBytes).retrieve()
                            : reqSpec.retrieve();
                    return responseSpec
                            .toEntity(byte[].class)
                            .timeout(Duration.ofSeconds(service.timeoutSeconds()));
                })
                .flatMap(entity -> {
                    double ms = (System.nanoTime() - start) / 1_000_000.0;
                    log.info("{} {} -> {} {}ms [{}]",
                            request.method(), request.path(),
                            entity.getStatusCode().value(), String.format("%.1f", ms), targetUrl);

                    ServerResponse.BodyBuilder responseBuilder =
                            ServerResponse.status(entity.getStatusCode());

                    entity.getHeaders().forEach((name, values) -> {
                        if (!SKIP_RESPONSE_HEADERS.contains(name.toLowerCase())) {
                            values.forEach(v -> responseBuilder.header(name, v));
                        }
                    });

                    byte[] body = entity.getBody();
                    if (body != null && body.length > 0) {
                        return responseBuilder.bodyValue(body);
                    }
                    return responseBuilder.build();
                })
                .onErrorResume(ex -> {
                    double ms = (System.nanoTime() - start) / 1_000_000.0;
                    if (ex instanceof java.util.concurrent.TimeoutException) {
                        log.error("{} {} -> 504 {}ms [{}] timeout after {}s",
                                request.method(), request.path(), String.format("%.1f", ms),
                                targetUrl, service.timeoutSeconds());
                        return ServerResponse.status(HttpStatus.GATEWAY_TIMEOUT)
                                .bodyValue(new ErrorBody("Request timed out after " + service.timeoutSeconds() + "s"));
                    }
                    log.error("{} {} -> 502 {}ms [{}] error: {}",
                            request.method(), request.path(), String.format("%.1f", ms), targetUrl, ex.getMessage());
                    return ServerResponse.status(HttpStatus.BAD_GATEWAY)
                            .bodyValue(new ErrorBody("Upstream unreachable: " + serviceName));
                });
    }

    record ErrorBody(String error) {}
}
