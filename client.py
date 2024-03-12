import socket
import cv2
import numpy as np

RECV_BUFFER = 1024


def main():
    server_addr = input("Enter server address: ")

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect((server_addr, 7020))

    while True:
        # keep receiving data from socket until all data is received
        data = b""
        while True:
            packet = sock.recv(RECV_BUFFER)
            data += packet
            if len(packet) < RECV_BUFFER:
                break

        # decode to a jpg image using opencv
        image = cv2.imdecode(np.frombuffer(data, np.uint8), cv2.IMREAD_COLOR)

        # show image
        if image is not None and image.shape[0] > 0:
            cv2.imshow("image", image)

        if cv2.waitKey(1) & 0xFF == ord("q"):
            break


if __name__ == "__main__":
    main()
