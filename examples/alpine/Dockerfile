FROM alpine:latest

RUN apk add lighttpd curl && rm -rf /var/cache/apk/*

COPY ./website /var/www/localhost/htdocs

HEALTHCHECK --interval=1m --timeout=1s \
  CMD curl -f http://localhost/ || exit 1

EXPOSE 80

ENTRYPOINT ["/usr/sbin/lighttpd", "-D", "-f", "/etc/lighttpd/lighttpd.conf"]
