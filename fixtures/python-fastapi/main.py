import os

from fastapi import FastAPI

app = FastAPI()
DATABASE_URL = os.environ.get("DATABASE_URL")
