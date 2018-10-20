# This script takes care of building your crate and packaging it for release

set -ex

main() {
    local src=$(pwd) \
          stage=

    case $TRAVIS_OS_NAME in
        linux)
            stage=$(mktemp -d)
            ;;
        osx)
            stage=$(mktemp -d -t tmp)
            ;;
    esac

    test -f Cargo.lock || cargo generate-lockfile

    for i in 2inbox dump_reading_list taskpaper; do  
       cross rustc --bin ${i} --target $TARGET --release -- -C lto
       cp target/$TARGET/release/${i} $stage/
    done

    cd $stage
    tar czf $src/$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    cd $src


    rm -rf $stage
}

main
