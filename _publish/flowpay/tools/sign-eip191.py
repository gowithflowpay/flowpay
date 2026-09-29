import base64
import os

from eth_account import Account
from eth_account.messages import encode_defunct


message = base64.b64decode(os.environ["MESSAGE_BASE64"]).decode("utf-8")
signature = Account.sign_message(
    encode_defunct(text=message),
    private_key=os.environ["PRIVATE_KEY"],
)
print(signature.signature.hex())
