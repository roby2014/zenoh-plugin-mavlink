import time
import zenoh
import argparse

def main(conf):
    # initiate logging
    zenoh.init_log_from_env_or("debug")
    key_out = '@/*/@mavlink/v2/out'

    print("Opening session...")
    with zenoh.open(conf) as session:

        print(f"Declaring Subscriber on '{key_out}'...")

        def listener(sample: zenoh.Sample):
            print(
                f">> [Subscriber] Received {sample.kind} ('{sample.key_expr}': '{sample.payload}')"
            )

        session.declare_subscriber(key_out, listener)

        print("Press CTRL-C to quit...")
        while True:
            time.sleep(1)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(prog="z_sub", description="zenoh sub example")
    parser.add_argument(
        "--config",
        "-c",
        dest="config",
        metavar="FILE",
        type=str,
        help="zenoh configuration file.",
    )

    args = parser.parse_args()
    conf = (
        zenoh.Config.from_file(args.config)
        if args.config is not None
        else zenoh.Config()
    )
    
    main(conf)