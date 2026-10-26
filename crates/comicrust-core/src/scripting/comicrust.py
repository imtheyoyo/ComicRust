# -*- coding: utf-8 -*-
"""Module « comicrust » : l'API offerte aux scripts Python de ComicRust.

Un script ne modifie jamais lui-même la bibliothèque ni les fichiers : il *demande* les
modifications (champs de fiche, renommages, écriture des informations dans le fichier) ;
ComicRust les vérifie, les montre en aperçu (scripts lancés depuis le menu) puis les
applique avec ses protections. Lire des fichiers et accéder au réseau reste libre.

Exemple :

    #@Name Nettoyer les séries
    #@Hook Books
    #@Description Retire les espaces en trop dans le nom des séries.
    import comicrust

    def nettoyer(books):
        for i, book in enumerate(books):
            if comicrust.cancelled():
                break
            comicrust.progress(i / len(books), book.series or "")
            propre = " ".join((book.series or "").split())
            if propre != book.series:
                book.series = propre        # mis en attente, pas écrit tout de suite

Ce fichier est fourni par ComicRust (il est réécrit à chaque lancement) : ne le modifiez pas.
"""

import _thread
import inspect
import io
import json
import math
import os
import queue
import re
import string
import sys
import threading
import traceback

__all__ = [
    "Book", "BookList", "Settings", "Cancelled", "ScriptError",
    "books", "book", "args", "hook", "interactive",
    "rename_file", "write_info", "flush",
    "progress", "cancelled", "check_cancelled", "log", "message",
    "ask", "ask_choice", "confirm", "settings", "format_name",
    "http_get", "http_get_text", "http_get_json",
]

PROTOCOL_VERSION = 1
# Nombre de modifications en attente au-delà duquel elles sont envoyées à ComicRust.
FLUSH_EVERY = 500
# Livres lus par lot.
BATCH = 500
# Délai (secondes) laissé au script pour s'arrêter de lui-même après une annulation.
INTERRUPT_DELAY = 1.0
# ComicRust a disparu (stdin fermé) : délai avant l'arrêt immédiat du processus, si le
# script ne s'est pas arrêté de lui-même après l'interruption.
EXIT_DELAY = 5.0


class Cancelled(KeyboardInterrupt):
    """Levée par :func:`check_cancelled` quand l'utilisateur a demandé l'arrêt.

    Dérive de ``KeyboardInterrupt`` : un ``except Exception`` du script ne l'avale pas.
    """


class ScriptError(Exception):
    """Demande refusée par ComicRust (livre hors contexte, réglages non enregistrés…)."""


# ---------------------------------------------------------------------------------------
# Canal de dialogue avec ComicRust
# ---------------------------------------------------------------------------------------

def _json_default(o):
    # Valeurs non JSON (dates…) : leur texte.
    return str(o)


class _Channel:
    def __init__(self, out):
        self._out = out
        self._lock = threading.Lock()
        self._ids = 0
        self._waiting = {}
        self._wlock = threading.Lock()
        self.cancelled = threading.Event()
        self.closed = False
        self.started = False
        # Message de fin envoyé : plus rien à interrompre.
        self.finished = False
        self.inbox = queue.Queue()

    def send(self, msg):
        # allow_nan=False : NaN et l'infini ne sont pas du JSON (ComicRust refuserait tout
        # le message) ; les modifications sont vérifiées avant (Book.set).
        data = (json.dumps(msg, ensure_ascii=False, default=_json_default, allow_nan=False) + "\n").encode("utf-8")
        with self._lock:
            self._out.write(data)
            self._out.flush()

    def request(self, method, **params):
        with self._wlock:
            self._ids += 1
            rid = self._ids
            box = queue.Queue(1)
            self._waiting[rid] = box
        msg = {"t": "req", "id": rid, "m": method}
        msg.update(params)
        self.send(msg)
        while True:
            try:
                rep = box.get(timeout=0.2)
                break
            except queue.Empty:
                if self.closed:
                    raise Cancelled("ComicRust a interrompu le dialogue")
        if rep.get("ok"):
            return rep.get("value")
        raise ScriptError(rep.get("error") or "demande refusée")

    def reader(self, inp):
        try:
            for raw in inp:
                try:
                    msg = json.loads(raw.decode("utf-8"))
                except ValueError:
                    continue
                t = msg.get("t")
                if t == "reply":
                    box = self._waiting.pop(msg.get("id"), None)
                    if box is not None:
                        box.put(msg)
                elif t == "cancel":
                    if not self.cancelled.is_set():
                        self.cancelled.set()
                        # Le script a INTERRUPT_DELAY secondes pour s'arrêter de lui-même
                        # (cancelled()) ; ensuite KeyboardInterrupt dans le fil principal.
                        t = threading.Timer(INTERRUPT_DELAY, self._interrupt)
                        t.daemon = True
                        t.start()
                else:
                    self.inbox.put(msg)
        finally:
            # ComicRust a fermé stdin (fin normale, arrêt forcé, ComicRust fermé ou planté) :
            # plus personne pour répondre. Si le script tourne encore, il est arrêté :
            # interruption du fil principal, puis sortie immédiate du processus après
            # EXIT_DELAY (un script bloqué dans un appel système ne reste pas orphelin).
            self.closed = True
            self.inbox.put(None)
            if not self.finished:
                self.cancelled.set()
                self._interrupt()
                t = threading.Timer(EXIT_DELAY, self._exit)
                t.daemon = True
                t.start()

    def _interrupt(self):
        if self.started and not self.finished:
            _thread.interrupt_main()

    def _exit(self):
        if not self.finished:
            os._exit(3)


class _Session:
    def __init__(self, ch, run):
        self.ch = ch
        self.run = run
        self.ids = list(run.get("book_ids") or [])
        self.id_set = set(self.ids)
        fields = run.get("fields") or {}
        self.editable = set(fields.get("editable") or [])
        self.read_only = set(fields.get("read_only") or [])
        self.custom_prefix = fields.get("custom_prefix") or "CustomValue:"
        known = self.editable | self.read_only
        self.snake = {_snake(f): f for f in known}
        self.cache = {}
        self.pending = []
        self.book_list = None
        st = run.get("settings") or {}
        self.settings = Settings(st.get("values") or {}, st.get("secret") or [])


_S = None


def _session():
    if _S is None:
        raise RuntimeError("le module comicrust n'est utilisable que dans un script lancé par ComicRust")
    return _S


def _snake(name):
    """``LanguageISO`` → ``language_iso``, ``CoverArtist`` → ``cover_artist``."""
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", name)
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s)
    return s.lower()


def _field(name):
    """Nom ComicRack d'un champ donné en ComicRack (``Series``) ou en snake_case (``series``)."""
    s = _session()
    if name in s.editable or name in s.read_only:
        return name
    if name.startswith(s.custom_prefix) and len(name) > len(s.custom_prefix):
        return name
    f = s.snake.get(name)
    if f is None:
        f = s.snake.get(_snake(name))
    if f is None:
        raise AttributeError("champ inconnu : %r" % (name,))
    return f


def _queue(item):
    s = _session()
    s.pending.append(item)
    if len(s.pending) >= FLUSH_EVERY:
        flush()


def _custom_values(store):
    """Décode ``CustomValuesStore`` (``,clé=valeur,…``, ``&#61;`` = ``=``, ``&#44;`` = ``,``)."""
    out = []
    for seg in (store or "").split(","):
        parts = seg.split("=")
        if len(parts) == 2:
            dec = lambda t: t.replace("&#61;", "=").replace("&#44;", ",")
            out.append((dec(parts[0]), dec(parts[1])))
    return out


def _book_id(book):
    return book.id if isinstance(book, Book) else str(book)


# ---------------------------------------------------------------------------------------
# Livres
# ---------------------------------------------------------------------------------------

class Book:
    """Un livre de la bibliothèque.

    Lecture : ``book.series`` ou ``book["Series"]`` (noms snake_case ou ComicRack) ;
    champ vide → ``None``. ``book.id`` : identifiant ; ``book.file`` : chemin du fichier.

    Écriture : ``book.series = "X"``, ``book["Series"] = "X"`` ou ``book.set("Series", "X")``.
    La modification est **mise en attente** et envoyée à ComicRust par lots ; elle est
    visible tout de suite dans l'objet. ``None`` vide le champ.
    """

    __slots__ = ("_data", "_changed")

    def __init__(self, data):
        object.__setattr__(self, "_data", dict(data))
        object.__setattr__(self, "_changed", {})

    @property
    def id(self):
        """Identifiant (GUID) du livre."""
        return self._data.get("Id")

    @property
    def file(self):
        """Chemin complet du fichier (``None`` pour une fiche sans fichier)."""
        return self._data.get("File")

    @property
    def changes(self):
        """Champs modifiés par le script (nom ComicRack → nouvelle valeur)."""
        return dict(self._changed)

    def get(self, field, default=None):
        """Valeur d'un champ (``default`` s'il est vide)."""
        if field in ("Id", "id"):
            return self.id
        if field in ("File", "file"):
            return self.file
        v = self._data.get(_field(field))
        return default if v is None else v

    def set(self, field, value):
        """Met en attente la modification d'un champ (``None`` = vider)."""
        s = _session()
        f = _field(field)
        if f not in s.editable and not f.startswith(s.custom_prefix):
            raise AttributeError("champ non modifiable : %s" % f)
        if isinstance(value, (list, tuple, set)):
            value = ", ".join(str(v) for v in value)
        elif value is not None and not isinstance(value, (str, int, float, bool)):
            value = str(value)
        if isinstance(value, float) and not math.isfinite(value):
            # Refus de cette seule modification (le reste du script continue s'il
            # intercepte l'erreur) : NaN / infini ne sont pas des valeurs de fiche.
            raise ValueError("valeur %r refusée pour le champ %s : nombre non fini (NaN ou infini)" % (value, f))
        if f.startswith(s.custom_prefix):
            # Valeur personnalisée : gardée à part (lue par custom_value), la fiche
            # n'a pas de champ de ce nom.
            self._changed[f] = value
            _queue({"op": "set", "book": self.id, "field": f, "value": value})
            return
        if self._data.get(f) == value and f not in self._changed:
            return
        self._data[f] = value
        self._changed[f] = value
        _queue({"op": "set", "book": self.id, "field": f, "value": value})

    def custom_value(self, key, default=None):
        """Valeur personnalisée ``key`` (sans casse, comme ``GetCustomValue`` de ComicRack)."""
        wanted = (_session().custom_prefix + key).lower()
        for f, pending in reversed(list(self._changed.items())):
            if f.lower() == wanted:
                return default if pending in (None, "") else pending
        for k, v in _custom_values(self._data.get("CustomValuesStore")):
            if k.lower() == key.lower():
                return v
        return default

    def set_custom_value(self, key, value):
        """Met en attente une valeur personnalisée (``None`` ou ``""`` la supprime)."""
        if not key:
            raise ValueError("clé de valeur personnalisée vide")
        self.set(_session().custom_prefix + key, None if value is None else str(value))

    def to_dict(self):
        """Copie de la fiche (noms ComicRack)."""
        return dict(self._data)

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        return self._data.get(_field(name))

    def __setattr__(self, name, value):
        if name.startswith("_"):
            object.__setattr__(self, name, value)
        else:
            self.set(name, value)

    def __getitem__(self, field):
        return self.get(field)

    def __setitem__(self, field, value):
        self.set(field, value)

    def __eq__(self, other):
        return isinstance(other, Book) and other.id == self.id

    def __hash__(self):
        return hash(self.id)

    def __repr__(self):
        d = self._data
        return "<Book %s %r #%s>" % (self.id, d.get("Series") or d.get("File"), d.get("Number") or "")


class BookList:
    """Livres du contexte du script, lus par lots à la demande (une grosse bibliothèque ne
    coûte que ce qui est parcouru). Se comporte comme une liste en lecture : ``len()``,
    ``books[i]``, ``for book in books``."""

    def __init__(self, ids):
        self._ids = list(ids)

    def __len__(self):
        return len(self._ids)

    def _load(self, start):
        s = _session()
        chunk = [i for i in self._ids[start:start + BATCH] if i not in s.cache]
        if chunk:
            for d in s.ch.request("books", ids=chunk) or []:
                b = Book(d)
                s.cache[b.id] = b

    def __getitem__(self, i):
        if isinstance(i, slice):
            return [self[j] for j in range(*i.indices(len(self._ids)))]
        if i < 0:
            i += len(self._ids)
        bid = self._ids[i]
        s = _session()
        if bid not in s.cache:
            self._load(i - i % BATCH)
        return s.cache.get(bid)

    def __iter__(self):
        s = _session()
        for start in range(0, len(self._ids), BATCH):
            self._load(start)
            for bid in self._ids[start:start + BATCH]:
                b = s.cache.get(bid)
                if b is not None:  # livre disparu entre-temps
                    yield b

    def __bool__(self):
        return bool(self._ids)

    def __repr__(self):
        return "<BookList de %d livres>" % len(self._ids)


def books():
    """Livres sur lesquels porte le script : sélection (hook ``Books``), toute la
    bibliothèque (``Library``), livre ouvert/fermé (``BookOpened``/``BookClosed``),
    nouveaux livres (``NewBooks``) ; vide pour ``Startup`` et ``ConfigScript``."""
    s = _session()
    if s.book_list is None:
        s.book_list = BookList(s.ids)
    return s.book_list


def book():
    """Premier livre du contexte (le livre ouvert pour ``BookOpened``), ou ``None``."""
    b = books()
    return b[0] if len(b) else None


def args():
    """Arguments libres transmis par ComicRust (``None`` le plus souvent)."""
    return _session().run.get("args")


def hook():
    """Nom du hook qui a déclenché le script (``"Books"``, ``"NewBooks"``…)."""
    return _session().run.get("hook")


def interactive():
    """Vrai si l'utilisateur peut répondre aux questions (faux pour un automatisme)."""
    return bool(_session().run.get("interactive"))


# ---------------------------------------------------------------------------------------
# Modifications
# ---------------------------------------------------------------------------------------

def rename_file(book, new_path):
    """Demande de renommer/déplacer le fichier du livre. ``new_path`` : chemin complet, ou
    simple nom (même dossier). ComicRust vérifie la destination avant d'appliquer."""
    _queue({"op": "rename", "book": _book_id(book), "path": str(new_path)})


def write_info(book):
    """Demande d'écrire la fiche (ComicInfo.xml) dans le fichier du livre, après
    application des modifications de champs."""
    _queue({"op": "write_info", "book": _book_id(book)})


def flush():
    """Envoie tout de suite les modifications en attente (fait automatiquement par lots
    et à la fin du script). Renvoie le nombre de modifications acceptées ; les refus sont
    écrits dans le journal."""
    s = _session()
    if not s.pending:
        return 0
    items, s.pending = s.pending, []
    rep = s.ch.request("changes", items=items) or {}
    for r in rep.get("rejected") or []:
        i = r.get("index")
        what = items[i] if isinstance(i, int) and 0 <= i < len(items) else "?"
        sys.stderr.write("comicrust : modification refusée (%s) : %r\n" % (r.get("reason"), what))
    if rep.get("live_error"):
        log("modifications non appliquées : %s" % rep.get("live_error"), level="error")
    return int(rep.get("accepted") or 0)


# ---------------------------------------------------------------------------------------
# Interface : progression, journal, questions
# ---------------------------------------------------------------------------------------

def progress(fraction=None, text=""):
    """Avancement : ``fraction`` entre 0 et 1 (``None`` = inconnu), ``text`` affiché."""
    if fraction is not None:
        try:
            fraction = float(fraction)
            fraction = max(0.0, min(1.0, fraction)) if math.isfinite(fraction) else None
        except (TypeError, ValueError):
            fraction = None
    _session().ch.send({"t": "progress", "fraction": fraction, "text": str(text)})


def cancelled():
    """Vrai si l'utilisateur a demandé l'arrêt : à tester régulièrement dans les boucles."""
    return _session().ch.cancelled.is_set()


def check_cancelled():
    """Lève :class:`Cancelled` si l'utilisateur a demandé l'arrêt."""
    if cancelled():
        raise Cancelled()


def log(*values, level="info"):
    """Écrit dans le journal du script (``level`` : ``"info"``, ``"warning"``, ``"error"``).
    ``print()`` écrit aussi dans le journal."""
    _session().ch.send({"t": "log", "level": level, "text": " ".join(str(v) for v in values)})


def message(text):
    """Affiche un message à l'utilisateur (dans le journal pour un automatisme)."""
    _session().ch.request("message", text=str(text))


def ask(question, default="", secret=False):
    """Demande un texte à l'utilisateur ; ``default`` sans interface ou si annulé.

    ``secret=True`` (clé d'API, mot de passe…) : saisie masquée, ``default`` n'est **ni
    affiché ni envoyé** à l'interface ; une réponse vide rend ``default`` (« garder la
    valeur actuelle »)."""
    if secret:
        v = _session().ch.request("ask", kind="text", question=str(question), default="", secret=True)
        return default if v in (None, "") else v
    v = _session().ch.request("ask", kind="text", question=str(question), default=default)
    return default if v is None else v


def ask_choice(question, choices, default=None):
    """Demande de choisir dans ``choices`` (liste de textes) ; renvoie le choix ou
    ``default`` (sans interface ou si annulé)."""
    choices = [str(c) for c in choices]
    return _session().ch.request("ask", kind="choice", question=str(question), choices=choices, default=default)


def confirm(question, default=False):
    """Question oui/non ; ``default`` sans interface."""
    return bool(_session().ch.request("ask", kind="confirm", question=str(question), default=bool(default)))


# ---------------------------------------------------------------------------------------
# Réglages
# ---------------------------------------------------------------------------------------

class Settings(dict):
    """Réglages propres au script (communs à toutes les fonctions du fichier), conservés
    par ComicRust. Dictionnaire ordinaire, plus :

    - ``mark_secret(clé)`` : valeur secrète (clé d'API…), stockée chiffrée par ComicRust
      (sous Windows ; ailleurs elle n'est pas conservée) ;
    - ``save()`` : enregistre (fait aussi automatiquement à la fin si modifié).
    """

    def __init__(self, values, secret):
        super().__init__(values)
        self.secret = set(secret)
        self._saved = json.dumps(values, sort_keys=True, default=_json_default)

    def mark_secret(self, key, secret=True):
        if secret:
            self.secret.add(key)
        else:
            self.secret.discard(key)

    def is_secret(self, key):
        return key in self.secret

    def _dirty(self):
        return json.dumps(dict(self), sort_keys=True, default=_json_default) != self._saved

    def save(self):
        _session().ch.request("save_settings", values=dict(self), secret=sorted(self.secret))
        self._saved = json.dumps(dict(self), sort_keys=True, default=_json_default)


def settings():
    """Réglages du script (voir :class:`Settings`)."""
    return _session().settings


# ---------------------------------------------------------------------------------------
# Outils
# ---------------------------------------------------------------------------------------

_INVALID = re.compile(r'[<>:"/\\|?*\x00-\x1f]')


class _NameFormatter(string.Formatter):
    def __init__(self, book, replace):
        super().__init__()
        self.book = book
        self.replace = replace

    def get_value(self, key, a, kw):
        if isinstance(key, int):
            return a[key]
        if key in kw:
            return kw[key]
        try:
            v = self.book.get(key)
        except AttributeError:
            v = None
        if v is None:
            return ""
        if isinstance(v, str):
            return _INVALID.sub(self.replace, v).strip()
        return v

    def format_field(self, value, spec):
        if spec and isinstance(value, str):
            try:  # « {number:03} » avec un numéro texte
                num = float(value)
                value = int(num) if num.is_integer() else num
            except ValueError:
                pass
        try:
            return super().format_field(value, spec)
        except (ValueError, TypeError):
            return str(value)


def format_name(template, book, replace="_", **extra):
    """Construit un nom de fichier : ``format_name("{series} #{number:03} ({year})", book)``.

    Les champs (snake_case ou ComicRack) vides donnent ``""`` ; les caractères interdits
    dans un nom de fichier (``<>:"/\\|?*``) des valeurs sont remplacés par ``replace``
    (les ``/`` et ``\\`` du modèle lui-même sont gardés : ``"{publisher}/{series}"``).
    Les espaces doublés sont réduits. ``extra`` : valeurs supplémentaires."""
    text = _NameFormatter(book, replace).format(template, **extra)
    return re.sub(r"[ \t]{2,}", " ", text).strip()


_UA = "ComicRust-scripts/1.0"


def http_get(url, headers=None, timeout=30, data=None):
    """Télécharge ``url`` et renvoie les octets (``data`` : corps d'un POST).

    Utilise ``urllib`` avec le contexte SSL par défaut : sous Windows, Python lit les
    certificats racine du magasin Windows. Un ``User-Agent`` est ajouté (Comic Vine refuse
    celui de Python par défaut)."""
    import ssl
    import urllib.request
    h = {"User-Agent": _UA}
    h.update(headers or {})
    req = urllib.request.Request(url, data=data, headers=h)
    ctx = ssl.create_default_context()
    with urllib.request.urlopen(req, timeout=timeout, context=ctx) as r:
        return r.read()


def http_get_text(url, headers=None, timeout=30, encoding="utf-8"):
    """Comme :func:`http_get`, décodé en texte."""
    return http_get(url, headers, timeout).decode(encoding, "replace")


def http_get_json(url, headers=None, timeout=30):
    """Comme :func:`http_get`, décodé en JSON."""
    return json.loads(http_get(url, headers, timeout).decode("utf-8"))


# ---------------------------------------------------------------------------------------
# Lancement (appelé par ComicRust, pas par les scripts)
# ---------------------------------------------------------------------------------------

def _load_script(path):
    import importlib.util
    stem = re.sub(r"\W", "_", os.path.splitext(os.path.basename(path))[0])
    spec = importlib.util.spec_from_file_location("comicrust_script_" + stem, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


def _call(func, hook_name):
    """Appelle la fonction : sans argument si elle n'en prend pas, avec le livre pour
    ``BookOpened``/``BookClosed``, avec la liste des livres sinon."""
    try:
        kinds = (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD,
                 inspect.Parameter.VAR_POSITIONAL)
        wants = any(p.kind in kinds for p in inspect.signature(func).parameters.values())
    except (TypeError, ValueError):
        wants = True
    if not wants:
        return func()
    if hook_name in ("BookOpened", "BookClosed"):
        return func(book())
    return func(books())


def _result_value(v):
    try:
        json.dumps(v, allow_nan=False)
        return v
    except (TypeError, ValueError):
        return repr(v)


def _format_error(e):
    """Traceback sans les cadres du module comicrust."""
    tb = e.__traceback__
    here = os.path.abspath(__file__)
    while tb is not None and os.path.abspath(tb.tb_frame.f_code.co_filename) == here:
        tb = tb.tb_next
    return "".join(traceback.format_exception(type(e), e, tb))


def _main():
    global _S
    # Canal du protocole : la sortie standard d'origine. Ensuite, le descripteur 1 et
    # sys.stdout vont vers stderr (journal) : un print() ne casse pas le protocole.
    proto_out = os.fdopen(os.dup(1), "wb")
    proto_in = os.fdopen(os.dup(0), "rb")
    os.dup2(2, 1)
    null = os.open(os.devnull, os.O_RDONLY)
    os.dup2(null, 0)
    os.close(null)
    sys.stdout = sys.stderr
    sys.stdin = io.StringIO("")

    ch = _Channel(proto_out)
    threading.Thread(target=ch.reader, args=(proto_in,), daemon=True, name="comicrust").start()
    ch.send({"t": "hello", "protocol": PROTOCOL_VERSION, "python": sys.version.split()[0]})
    run = ch.inbox.get()
    if not run or run.get("t") != "run":
        os._exit(2)
    _S = _Session(ch, run)
    script = run["script"]
    for d in reversed([os.path.dirname(os.path.abspath(script))] + list(run.get("lib_dirs") or [])):
        if d not in sys.path:
            sys.path.insert(0, d)

    end = None
    try:
        if ch.cancelled.is_set():
            raise Cancelled()
        ch.started = True
        mod = _load_script(script)
        func = getattr(mod, run["function"], None)
        if not callable(func):
            raise NameError("fonction « %s » introuvable dans %s" % (run["function"], os.path.basename(script)))
        value = _call(func, run.get("hook"))
        ch.started = False
        flush()
        if _S.settings._dirty():
            _S.settings.save()
        end = {"t": "done", "value": _result_value(value)}
    except KeyboardInterrupt as e:
        ch.started = False
        if ch.cancelled.is_set():
            try:
                if not ch.closed:
                    flush()
            except BaseException:
                pass
            end = {"t": "cancelled"}
        else:
            end = {"t": "error", "type": type(e).__name__, "message": str(e), "traceback": _format_error(e)}
    except SystemExit as e:
        ch.started = False
        if e.code in (None, 0):
            try:
                flush()
                if _S.settings._dirty():
                    _S.settings.save()
                end = {"t": "done", "value": None}
            except BaseException as e2:
                end = {"t": "error", "type": type(e2).__name__, "message": str(e2), "traceback": _format_error(e2)}
        else:
            end = {"t": "error", "type": "SystemExit", "message": "sortie avec le code %r" % (e.code,),
                   "traceback": ""}
    except BaseException as e:
        ch.started = False
        end = {"t": "error", "type": type(e).__name__, "message": str(e), "traceback": _format_error(e)}
        # Les modifications valides demandées avant l'erreur restent proposées (aperçu
        # partiel), comme celles déjà envoyées.
        try:
            if not ch.closed:
                flush()
        except BaseException:
            pass
    ch.finished = True
    try:
        ch.send(end)
    except BaseException:
        pass
    try:
        sys.stderr.flush()
    except BaseException:
        pass
    # Fils lancés par le script : on ne les attend pas.
    os._exit(0)
