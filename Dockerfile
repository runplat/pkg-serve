FROM mcr.microsoft.com/cbl-mariner/base/rust:1.72 as build
COPY . $PWD
RUN tdnf update -y & tdnf install pkgconfig openssl-devel -y
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN rustup update
RUN cargo build --release

FROM mcr.microsoft.com/cbl-mariner/base/core:2.0
COPY --from=build target/release/pkg-serve ./
COPY entrypoint.sh entrypoint.sh
RUN tdnf update -y & tdnf install ca-certificates -y
RUN chmod +x ./pkg-serve
RUN chmod +x ./entrypoint.sh
ENTRYPOINT [ "/entrypoint.sh" ]
