# This script takes care of testing
set -ex

main() {
    if [ ! -z $DISABLE_TESTS ]; then
        return
    fi

    cross test --release --target $TARGET
    cross build --release --target $TARGET
    mkdir -p target/release
    cp target/$TARGET/release/sk target/release

    case $TARGET in
        x86_64-unknown-linux-gnu|i686-unknown-linux-gnu|x86_64-unknown-linux-musl)
            # run the integration test
            tmux new "python3 test/test_skim.py &> out && touch ok" && cat out && [ -e ok ]
            ;;
        x86_64-apple-darwin|i686-apple-darwin)
            # run the integration test
            tmux new "python3 test/test_skim.py &> out && touch ok" && cat out && [ -e ok ]
            ;;
        *)
            ;;
    esac
}

if [ -z $TRAVIS_TAG ]; then
    main
fi
