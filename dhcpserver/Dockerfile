FROM rust:1.47.0

WORKDIR /dhcp

COPY LeaseEntries.sql LeaseEntries.sql

RUN apt-get update && apt-get install -y --no-install-recommends \
  libsqlite3-dev \
  sqlite3 \
  && apt-get -y clean \
  && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["sqlite3"]
CMD ["shared/dhcp.db", "-init", "LeaseEntries.sql"]
