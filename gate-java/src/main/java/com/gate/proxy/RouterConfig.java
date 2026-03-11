package com.gate.proxy;

import com.gate.health.HealthHandler;
import com.gate.health.ServicesHandler;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.http.HttpMethod;
import org.springframework.web.reactive.function.server.RequestPredicates;
import org.springframework.web.reactive.function.server.RouterFunction;
import org.springframework.web.reactive.function.server.RouterFunctions;
import org.springframework.web.reactive.function.server.ServerResponse;

@Configuration
public class RouterConfig {

    @Bean
    public RouterFunction<ServerResponse> routes(GatewayHandler gateway, HealthHandler health, ServicesHandler services) {
        return RouterFunctions.route()
                .GET("/health", health)
                .GET("/services", services)
                .path("/{name}", b -> b
                        .GET("/**", gateway)
                        .POST("/**", gateway)
                        .PUT("/**", gateway)
                        .DELETE("/**", gateway)
                        .PATCH("/**", gateway)
                        .add(RouterFunctions.route(RequestPredicates.method(HttpMethod.HEAD).and(RequestPredicates.path("/**")), gateway))
                        .add(RouterFunctions.route(RequestPredicates.method(HttpMethod.OPTIONS).and(RequestPredicates.path("/**")), gateway)))
                .build();
    }
}
