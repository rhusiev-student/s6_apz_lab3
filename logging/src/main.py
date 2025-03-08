import asyncio
import logging
from hazelcaster import Hazelcaster

import argparse

from grpc import aio
from grpc_generated import logging_pb2_grpc
from log import Logger

logging.basicConfig()
logging.getLogger("hazelcast").setLevel(logging.ERROR)
logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)


async def serve(hazelcaster):
    with hazelcaster:
        srv = aio.server()
        logging_pb2_grpc.add_LoggingServiceServicer_to_server(Logger(hazelcaster), srv)

        address = "0.0.0.0:13228"
        srv.add_insecure_port(address)

        logger.info(f"Starting server on {address}")
        await srv.start()
        await srv.wait_for_termination()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("num", type=int)
    args = parser.parse_args()
    num: int = args.num

    hazelcaster = Hazelcaster(
        "logging",
        num
    )
    asyncio.run(serve(hazelcaster))
