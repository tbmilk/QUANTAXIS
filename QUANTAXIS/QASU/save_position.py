from pymongo import ASCENDING
from QUANTAXIS.QAUtil import DATABASE
"""对于POSITION的增删改查
"""


def save_position(message, collection=None):
    """save account

    Arguments:
        message {[type]} -- [description]

    Keyword Arguments:
        collection {[type]} -- [description] (default: {DATABASE})
    """
    collection = DATABASE.positions if collection is None else collection
    try:
        collection.create_index(
            [("account_cookie", ASCENDING), ("portfolio_cookie", ASCENDING), ("username", ASCENDING), ("position_id", ASCENDING)], unique=True)
    except Exception:
        pass
    collection.update_one(
        {'account_cookie': message['account_cookie'], 'position_id': message['position_id'],
            'portfolio_cookie': message['portfolio_cookie'], 'username': message['username']},
        {'$set': message},
        upsert=True
    )
