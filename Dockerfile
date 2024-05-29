FROM mcr.microsoft.com/cbl-mariner/base/rust:1.72 as build
COPY . $PWD
RUN rustup update stable
RUN cargo build --release

FROM mcr.microsoft.com/cbl-mariner/base/core:2.0
COPY --from=build target/release/pkg-serve ./
COPY entrypoint.sh entrypoint.sh
RUN chmod +x ./pkg-serve
RUN chmod +x ./entrypoint.sh
ENTRYPOINT [ "/entrypoint.sh" ]
