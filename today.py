
import datetime
from dateutil import relativedelta
import requests
import os
from lxml import etree
import time
import hashlib
from dotenv import load_dotenv

load_dotenv()

access_token = os.environ.get('ACCESS_TOKEN')
user_name = os.environ.get('USER_NAME', 'THARUN-BART')

if not access_token:
    print(
        "WARNING: ACCESS_TOKEN environment variable is not set. "
        "You must set this to your GitHub Personal Access Token."
    )
    access_token = "DUMMY_TOKEN"

HEADERS = {'authorization': 'token ' + access_token}
USER_NAME = os.environ.get('USER_NAME', 'THARUN-BART')

QUERY_COUNT = {
    'user_getter': 0,
    'follower_getter': 0,
    'graph_repos_stars': 0,
    'recursive_loc': 0,
    'graph_commits': 0,
    'loc_query': 0
}


# --------------------------------------------------
# GitHub GraphQL request helper with retry handling
# --------------------------------------------------

def graphql_request(query, variables, retries=5, timeout=60):
    url = 'https://api.github.com/graphql'
    transient_statuses = {429, 500, 502, 503, 504}

    for attempt in range(retries):
        try:
            response = requests.post(
                url,
                json={'query': query, 'variables': variables},
                headers=HEADERS,
                timeout=timeout
            )

            if response.status_code not in transient_statuses:
                return response

            if attempt == retries - 1:
                return response

            retry_after = response.headers.get('Retry-After')

            if retry_after and retry_after.isdigit():
                delay = int(retry_after)
            else:
                delay = min(2 ** attempt, 30)

            print(
                f"GitHub returned HTTP {response.status_code}. "
                f"Retrying in {delay}s "
                f"({attempt + 1}/{retries - 1})..."
            )

        except requests.RequestException as error:
            if attempt == retries - 1:
                raise

            delay = min(2 ** attempt, 30)

            print(
                f"GitHub request failed: {error}. "
                f"Retrying in {delay}s "
                f"({attempt + 1}/{retries - 1})..."
            )

        time.sleep(delay)

    raise RuntimeError('Unexpected retry loop exit')


def simple_request(func_name, query, variables):
    request = graphql_request(query, variables)

    if request.status_code == 200:
        result = request.json()

        if result.get('errors'):
            raise Exception(
                func_name,
                ' GraphQL request failed:',
                result['errors'],
                QUERY_COUNT
            )

        return request

    raise Exception(
        func_name,
        ' has failed with a',
        request.status_code,
        request.text,
        QUERY_COUNT
    )


# --------------------------------------------------
# Age calculation
# --------------------------------------------------

def daily_readme(birthday):
    """
    Returns the length of time since I was born.
    Example: 'XX years, XX months, XX days'
    """
    diff = relativedelta.relativedelta(
        datetime.datetime.today(), birthday
    )

    return '{} {}, {} {}, {} {}{}'.format(
        diff.years, 'year' + format_plural(diff.years),
        diff.months, 'month' + format_plural(diff.months),
        diff.days, 'day' + format_plural(diff.days),
        ' 🎂' if diff.months == 0 and diff.days == 0 else ''
    )


def format_plural(unit):
    return 's' if unit != 1 else ''


# --------------------------------------------------
# Contribution graph
# --------------------------------------------------

def graph_commits(start_date, end_date):
    query_count('graph_commits')

    query = '''
    query($start_date: DateTime!, $end_date: DateTime!, $login: String!) {
        user(login: $login) {
            contributionsCollection(from: $start_date, to: $end_date) {
                contributionCalendar {
                    totalContributions
                }
            }
        }
    }'''

    variables = {
        'start_date': start_date,
        'end_date': end_date,
        'login': USER_NAME
    }

    request = simple_request(
        graph_commits.__name__, query, variables
    )

    return int(
        request.json()['data']['user']
        ['contributionsCollection']['contributionCalendar']
        ['totalContributions']
    )


# --------------------------------------------------
# Repository and star counts
# --------------------------------------------------

def graph_repos_stars(
    count_type, owner_affiliation, cursor=None,
    add_loc=0, del_loc=0
):
    query_count('graph_repos_stars')

    query = '''
    query ($owner_affiliation: [RepositoryAffiliation], $login: String!, $cursor: String) {
        user(login: $login) {
            repositories(first: 100, after: $cursor, ownerAffiliations: $owner_affiliation) {
                totalCount
                edges {
                    node {
                        ... on Repository {
                            nameWithOwner
                            stargazers {
                                totalCount
                            }
                        }
                    }
                }
                pageInfo {
                    endCursor
                    hasNextPage
                }
            }
        }
    }'''

    variables = {
        'owner_affiliation': owner_affiliation,
        'login': USER_NAME,
        'cursor': cursor
    }

    request = simple_request(
        graph_repos_stars.__name__, query, variables
    )

    repositories = request.json()['data']['user']['repositories']

    if count_type == 'repos':
        return repositories['totalCount']

    if count_type == 'stars':
        return stars_counter(repositories['edges'])

    raise ValueError(f'Unknown count_type: {count_type}')


# --------------------------------------------------
# LOC calculation and pagination
# --------------------------------------------------

def recursive_loc(
    owner, repo_name, data, cache_comment,
    addition_total=0, deletion_total=0,
    my_commits=0, cursor=None
):
    query_count('recursive_loc')

    query = '''
    query ($repo_name: String!, $owner: String!, $cursor: String) {
        repository(name: $repo_name, owner: $owner) {
            defaultBranchRef {
                target {
                    ... on Commit {
                        history(first: 100, after: $cursor) {
                            totalCount
                            edges {
                                node {
                                    ... on Commit {
                                        committedDate
                                    }
                                    author {
                                        user {
                                            id
                                        }
                                    }
                                    deletions
                                    additions
                                }
                            }
                            pageInfo {
                                endCursor
                                hasNextPage
                            }
                        }
                    }
                }
            }
        }
    }'''

    variables = {
        'repo_name': repo_name,
        'owner': owner,
        'cursor': cursor
    }

    try:
        request = graphql_request(query, variables)
    except requests.RequestException:
        force_close_file(data, cache_comment)
        raise

    if request.status_code != 200:
        force_close_file(data, cache_comment)

        if request.status_code == 403:
            raise Exception(
                'GitHub rate or abuse limit reached!',
                request.text, QUERY_COUNT
            )

        raise Exception(
            'recursive_loc() has failed with a',
            request.status_code,
            request.text,
            QUERY_COUNT
        )

    result = request.json()

    if result.get('errors'):
        force_close_file(data, cache_comment)
        raise Exception(
            'GitHub GraphQL error:',
            result['errors'],
            QUERY_COUNT
        )

    repository = result.get('data', {}).get('repository')

    if repository is None:
        force_close_file(data, cache_comment)
        raise Exception(
            'Repository not found or inaccessible:',
            owner, repo_name
        )

    branch = repository.get('defaultBranchRef')

    if branch is None:
        return 0

    history = branch['target']['history']

    return loc_counter_one_repo(
        owner, repo_name, data, cache_comment,
        history, addition_total, deletion_total, my_commits
    )


def loc_counter_one_repo(
    owner, repo_name, data, cache_comment,
    history, addition_total, deletion_total, my_commits
):
    for node in history['edges']:
        if node['node']['author']['user'] == OWNER_ID:
            my_commits += 1
            addition_total += node['node']['additions']
            deletion_total += node['node']['deletions']

    if history['edges'] == [] or not history['pageInfo']['hasNextPage']:
        return addition_total, deletion_total, my_commits

    return recursive_loc(
        owner, repo_name, data, cache_comment,
        addition_total, deletion_total, my_commits,
        history['pageInfo']['endCursor']
    )


# --------------------------------------------------
# Repository listing for LOC calculation
# --------------------------------------------------

def loc_query(
    owner_affiliation, comment_size=0, force_cache=False,
    cursor=None, edges=None
):
    query_count('loc_query')

    if edges is None:
        edges = []

    query = '''
    query ($owner_affiliation: [RepositoryAffiliation], $login: String!, $cursor: String) {
        user(login: $login) {
            repositories(first: 60, after: $cursor, ownerAffiliations: $owner_affiliation) {
                edges {
                    node {
                        ... on Repository {
                            nameWithOwner
                            defaultBranchRef {
                                target {
                                    ... on Commit {
                                        history {
                                            totalCount
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                pageInfo {
                    endCursor
                    hasNextPage
                }
            }
        }
    }'''

    variables = {
        'owner_affiliation': owner_affiliation,
        'login': USER_NAME,
        'cursor': cursor
    }

    request = simple_request(loc_query.__name__, query, variables)

    repositories = (
        request.json()['data']['user']['repositories']
    )

    edges.extend(repositories['edges'])

    if repositories['pageInfo']['hasNextPage']:
        return loc_query(
            owner_affiliation,
            comment_size,
            force_cache,
            repositories['pageInfo']['endCursor'],
            edges
        )

    return cache_builder(edges, comment_size, force_cache)


# --------------------------------------------------
# Cache handling
# --------------------------------------------------

def cache_builder(
    edges, comment_size, force_cache,
    loc_add=0, loc_del=0
):
    cached = True

    filename = (
        'cache/'
        + hashlib.sha256(USER_NAME.encode('utf-8')).hexdigest()
        + '.txt'
    )

    os.makedirs('cache', exist_ok=True)

    try:
        with open(filename, 'r') as file:
            data = file.readlines()

    except FileNotFoundError:
        data = []

        if comment_size > 0:
            for _ in range(comment_size):
                data.append(
                    'This line is a comment block. '
                    'Write whatever you want here.\n'
                )

        with open(filename, 'w') as file:
            file.writelines(data)

    if len(data) - comment_size != len(edges) or force_cache:
        cached = False
        flush_cache(edges, filename, comment_size)

        with open(filename, 'r') as file:
            data = file.readlines()

    cache_comment = data[:comment_size]
    data = data[comment_size:]

    for index in range(len(edges)):
        repo_hash, commit_count, *__ = data[index].split()

        if (
            repo_hash
            == hashlib.sha256(
                edges[index]['node']['nameWithOwner']
                .encode('utf-8')
            ).hexdigest()
        ):
            try:
                current_commit_count = edges[index]['node'][
                    'defaultBranchRef'
                ]['target']['history']['totalCount']

                if int(commit_count) != current_commit_count:
                    owner, repo_name = (
                        edges[index]['node']['nameWithOwner'].split('/')
                    )

                    loc = recursive_loc(
                        owner, repo_name, data, cache_comment
                    )

                    data[index] = (
                        repo_hash + ' '
                        + str(current_commit_count) + ' '
                        + str(loc[2]) + ' '
                        + str(loc[0]) + ' '
                        + str(loc[1]) + '\n'
                    )

            except TypeError:
                data[index] = repo_hash + ' 0 0 0 0\n'

    # Write cache atomically to avoid leaving a half-written file.
    temporary_filename = filename + '.tmp'

    with open(temporary_filename, 'w') as file:
        file.writelines(cache_comment)
        file.writelines(data)

    os.replace(temporary_filename, filename)

    for line in data:
        loc = line.split()
        loc_add += int(loc[3])
        loc_del += int(loc[4])

    return [loc_add, loc_del, loc_add - loc_del, cached]


def flush_cache(edges, filename, comment_size):
    data = []

    try:
        with open(filename, 'r') as file:
            if comment_size > 0:
                data = file.readlines()[:comment_size]
    except FileNotFoundError:
        pass

    temporary_filename = filename + '.tmp'

    with open(temporary_filename, 'w') as file:
        file.writelines(data)

        for node in edges:
            repo_hash = hashlib.sha256(
                node['node']['nameWithOwner'].encode('utf-8')
            ).hexdigest()

            file.write(repo_hash + ' 0 0 0 0\n')

    os.replace(temporary_filename, filename)


def force_close_file(data, cache_comment):
    filename = (
        'cache/'
        + hashlib.sha256(USER_NAME.encode('utf-8')).hexdigest()
        + '.txt'
    )

    os.makedirs('cache', exist_ok=True)

    temporary_filename = filename + '.tmp'

    with open(temporary_filename, 'w') as file:
        file.writelines(cache_comment)
        file.writelines(data)

    os.replace(temporary_filename, filename)

    print(
        'There was an error while calculating LOC. '
        'The available cache data has been saved to:',
        filename
    )


# --------------------------------------------------
# Archived repository statistics
# --------------------------------------------------

def add_archive():
    try:
        with open('cache/repository_archive.txt', 'r') as file:
            data = file.readlines()

        old_data = data
        data = data[7:len(data) - 3]

        added_loc, deleted_loc, added_commits = 0, 0, 0
        contributed_repos = len(data)

        for line in data:
            repo_hash, total_commits, my_commits, *loc = line.split()

            added_loc += int(loc[0])
            deleted_loc += int(loc[1])

            if my_commits.isdigit():
                added_commits += int(my_commits)

        added_commits += int(old_data[-1].split()[4][:-1])

        return [
            added_loc,
            deleted_loc,
            added_loc - deleted_loc,
            added_commits,
            contributed_repos
        ]

    except FileNotFoundError:
        return [0, 0, 0, 0, 0]


# --------------------------------------------------
# Stars
# --------------------------------------------------

def stars_counter(data):
    total_stars = 0

    for node in data:
        total_stars += node['node']['stargazers']['totalCount']

    return total_stars


# --------------------------------------------------
# SVG formatting
# --------------------------------------------------

def svg_overwrite(
    filename, age_data, commit_data, star_data,
    repo_data, contrib_data, follower_data, loc_data
):
    """
    Update SVG values while preserving the static ASCII art.
    """
    tree = etree.parse(filename)
    root = tree.getroot()

    justify_format(root, 'age_data', age_data)
    justify_format(root, 'commit_data', commit_data, 22)
    justify_format(root, 'star_data', star_data, 14)
    justify_format(root, 'repo_data', repo_data, 6)
    justify_format(root, 'contrib_data', contrib_data)
    justify_format(root, 'follower_data', follower_data, 10)
    justify_format(root, 'loc_data', loc_data[2], 9)
    justify_format(root, 'loc_add', loc_data[0])
    justify_format(root, 'loc_del', loc_data[1], 7)

    tree.write(
        filename,
        encoding='utf-8',
        xml_declaration=True
    )


def justify_format(root, element_id, new_text, length=0):
    if isinstance(new_text, int):
        new_text = '{:,}'.format(new_text)

    new_text = str(new_text)

    find_and_replace(root, element_id, new_text)

    just_len = max(0, length - len(new_text))

    if just_len <= 2:
        dot_map = {0: '', 1: ' ', 2: '. '}
        dot_string = dot_map[just_len]
    else:
        dot_string = ' ' + ('.' * just_len) + ' '

    find_and_replace(root, f'{element_id}_dots', dot_string)


def find_and_replace(root, element_id, new_text):
    element = root.find(f".//*[@id='{element_id}']")

    if element is not None:
        element.text = new_text


# --------------------------------------------------
# Commit counter
# --------------------------------------------------

def commit_counter(comment_size):
    total_commits = 0

    filename = (
        'cache/'
        + hashlib.sha256(USER_NAME.encode('utf-8')).hexdigest()
        + '.txt'
    )

    with open(filename, 'r') as file:
        data = file.readlines()

    data = data[comment_size:]

    for line in data:
        total_commits += int(line.split()[2])

    return total_commits


# --------------------------------------------------
# User and follower data
# --------------------------------------------------

def user_getter(username):
    query_count('user_getter')

    query = '''
    query($login: String!) {
        user(login: $login) {
            id
            createdAt
        }
    }'''

    request = simple_request(
        user_getter.__name__, query, {'login': username}
    )

    user = request.json()['data']['user']

    return {'id': user['id']}, user['createdAt']


def follower_getter(username):
    query_count('follower_getter')

    query = '''
    query($login: String!) {
        user(login: $login) {
            followers {
                totalCount
            }
        }
    }'''

    request = simple_request(
        follower_getter.__name__,
        query,
        {'login': username}
    )

    return int(
        request.json()['data']['user']['followers']['totalCount']
    )


# --------------------------------------------------
# Utilities
# --------------------------------------------------

def query_count(funct_id):
    QUERY_COUNT[funct_id] += 1


def perf_counter(funct, *args):
    start = time.perf_counter()
    funct_return = funct(*args)

    return funct_return, time.perf_counter() - start


def formatter(query_type, difference, funct_return=False, whitespace=0):
    print('{:<23}'.format('   ' + query_type + ':'), end='')

    if difference > 1:
        print('{:>12}'.format('%.4f' % difference + ' s '))
    else:
        print('{:>12}'.format('%.4f' % (difference * 1000) + ' ms'))

    if whitespace:
        return '{:,}'.format(funct_return).ljust(whitespace)

    return funct_return


# --------------------------------------------------
# Main
# --------------------------------------------------

if __name__ == '__main__':
    os.makedirs('cache', exist_ok=True)

    print('Calculation times:')

    user_data, user_time = perf_counter(user_getter, USER_NAME)
    OWNER_ID, acc_date = user_data

    formatter('account data', user_time)

    birthday = datetime.datetime(2006, 11, 29)

    age_data, age_time = perf_counter(daily_readme, birthday)
    formatter('age calculation', age_time)

    total_loc, loc_time = perf_counter(
        loc_query,
        ['OWNER', 'COLLABORATOR', 'ORGANIZATION_MEMBER'],
        7
    )

    if total_loc[-1]:
        formatter('LOC (cached)', loc_time)
    else:
        formatter('LOC (no cache)', loc_time)

    commit_data, commit_time = perf_counter(commit_counter, 7)

    star_data, star_time = perf_counter(
        graph_repos_stars, 'stars', ['OWNER']
    )

    repo_data, repo_time = perf_counter(
        graph_repos_stars, 'repos', ['OWNER']
    )

    contrib_data, contrib_time = perf_counter(
        graph_repos_stars,
        'repos',
        ['OWNER', 'COLLABORATOR', 'ORGANIZATION_MEMBER']
    )

    follower_data, follower_time = perf_counter(
        follower_getter, USER_NAME
    )

    if OWNER_ID == {'id': 'MDQ6VXNlcjU3MzMxMTM0'}:
        archived_data = add_archive()

        for index in range(len(total_loc) - 1):
            total_loc[index] += archived_data[index]

        contrib_data += archived_data[-1]
        commit_data += int(archived_data[-2])

    for index in range(len(total_loc) - 1):
        total_loc[index] = '{:,}'.format(total_loc[index])

    svg_overwrite(
        'dark_mode.svg',
        age_data,
        commit_data,
        star_data,
        repo_data,
        contrib_data,
        follower_data,
        total_loc[:-1]
    )

    total_time = (
        user_time + age_time + loc_time + commit_time
        + star_time + repo_time + contrib_time + follower_time
    )

    print('Total function time:', '{:.4f} s'.format(total_time))

    print(
        'Total GitHub GraphQL API calls:',
        '{:>3}'.format(sum(QUERY_COUNT.values()))
    )

    for function_name, count in QUERY_COUNT.items():
        print('{:<28}'.format('   ' + function_name + ':'), count)
