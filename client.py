### Demo client script. NOT meant to be used in production.

import cv2
import numpy as np
from websockets.sync.client import connect

RECV_BUFFER = 1024


def main():
    server_addr = input("Enter server address: ")

    with connect(f"ws://{server_addr}:7020/ws") as ws:
        while True:
            # receive data from server
            data = ws.recv()

            if not isinstance(data, bytes):
                continue

            # decode to a jpg image using opencv
            image = cv2.imdecode(
                np.frombuffer(data, np.uint8), cv2.IMREAD_COLOR
            )

            # show image
            if image is not None and image.shape[0] > 0:
                cv2.imshow("image", image)

            if cv2.waitKey(1) & 0xFF == ord("q"):
                break


if __name__ == "__main__":
    main()
