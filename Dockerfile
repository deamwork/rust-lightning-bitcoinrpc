FROM jasongop/rust-wasm32:1.39.0-nightly as rustenv

RUN set -x \
  && apt-get update \
  && apt-get install -y python3 python3-pip \
  && pip3 install --upgrade pip==9.0.3 \
  && mkdir -p /output/{server,cli}

WORKDIR /lightning

# # Install pre-build dependencies
# RUN mkdir -p {cli,ln-manager,primitives,protocol,server,srml}/src \
#   && for D in */; do echo "fn main() {println!(\"if you see this, the build broke\")}" > $D/src/main.rs; done
#
# # server
# COPY ./server/Cargo.* server/
# COPY ./cli/Cargo.* cli/
# COPY ./ln-manager/Cargo.* ln-manager/
# COPY ./primitives/Cargo.* primitives/
# COPY ./protocol/Cargo.* protocol/
# COPY ./srml/Cargo.* srml/
# RUN set -x \
#   && source $HOME/.cargo/env \
#   && for D in */; do cd $D && cargo fetch && rm -f Cargo.{toml,lock} src/main.rs && cd ..; done

COPY . /lightning

RUN set -x \
  && cd test/integration \
  && pip3 install -r requirements.txt

ARG BUILD_TYPE=debug
ENV FINAL_TYPE=$BUILD_TYPE

# Build server
RUN set -x \
  && source $HOME/.cargo/env \
  && cd /lightning/server \
  && if [ $BUILD_TYPE == "release" ]; then cargo build --release; else cargo build; fi \
  && [ -d "target/$BUILD_TYPE" ] && cp -r "/lightning/server/target/$BUILD_TYPE/" /output/server/$BUILD_TYPE

# Build cli
RUN set -x \
  && source $HOME/.cargo/env \
  && cd /lightning/cli \
  && if [ $BUILD_TYPE == "release" ]; then cargo build --release; else cargo build; fi \
  && [ -d "target/$BUILD_TYPE" ] && cp -r "/lightning/cli/target/$BUILD_TYPE/" /output/cli/$BUILD_TYPE


FROM alpine:3.10

ARG BUILD_TYPE=debug
ENV FINAL_TYPE=$BUILD_TYPE

WORKDIR /app
COPY --from=rustenv /output .

RUN export PATH="$PATH:/app/cli/$VER"

CMD ["./server/$FINAL_TYPE/rustbolt"]
