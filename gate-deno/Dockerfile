FROM denoland/deno:alpine-2.3.1
WORKDIR /app
COPY main.ts .
RUN deno cache main.ts
USER deno
EXPOSE 8080
CMD ["deno", "run", "--allow-net", "--allow-env", "main.ts"]
