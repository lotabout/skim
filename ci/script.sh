# This script takes care of testing
set -ex

main() {
    cargo build --release --verbose

    if [ ! -z $DISABLE_TESTS ]; then
        return
    fi

    cargo test --verbose

    case $TRAVIS_OS_NAME in
        linux)
            # run the integration test
            tmux new "python3.6 test/test_skim.py > out && touch ok" && cat out && [ -e ok ]
            ;;
        osx)
            # run the integration test
            tmux new "python3.6 test/test_skim.py > out && touch ok" && cat out && [ -e ok ]
            ;;
        *)
            ;;
    esac
}

if [ -z $TRAVIS_TAG ]; then
    main
fi
