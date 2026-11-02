/**
 * French messages. Same keys as en.ts (a missing or extra key does not
 * compile). Typography: "vous", a non-breaking space ( ) before
 * « : ? ! % » and between a number and its unit, « » for quotes.
 */
import type { Messages } from "./en";

export const fr: Messages = {
  // --- Onglets ---------------------------------------------------------------
  // « Tableau de bord » désigne le site dashboard.codecarbon.io : l'onglet
  // principal s'appelle donc « Mesure ».
  "tab.dashboard": "Mesure",
  "tab.history": "Historique",
  "tab.settings": "Réglages",
  "tab.about": "À propos",

  // --- Commun ----------------------------------------------------------------
  "common.open_data_folder": "Ouvrir le dossier des données",
  "common.cancel": "Annuler",
  "common.advanced": "Options avancées",
  "common.starting_engine": "Démarrage du moteur de mesure…",

  // --- Mesure ----------------------------------------------------------------
  "dashboard.retry": "Réessayer",
  "dashboard.start": "Démarrer la mesure",
  "dashboard.stop": "Arrêter la mesure",
  "dashboard.starting": "Démarrage…",
  "dashboard.stopping": "Arrêt…",
  "dashboard.power": "Puissance",
  "dashboard.co2": "CO₂e",
  "dashboard.energy": "Énergie",
  "dashboard.duration": "Durée",
  "dashboard.chart_title": "Puissance, {minutes} dernières min",
  "dashboard.chart_label": "Puissance totale sur les {minutes} dernières minutes",
  "dashboard.chart_max": "max {value}",
  "dashboard.last_session": "Dernière session",
  "dashboard.average_power": "Puissance moyenne",
  "dashboard.grid_intensity": "Intensité du réseau",
  "dashboard.average": "moyenne · {breakdown}",
  "dashboard.per_hour": "{value} par heure",
  "dashboard.grid": "réseau : {value}",
  "dashboard.cpu_load": "Charge CPU {value} %",
  "dashboard.uploading_to": "Envoi vers {target}",
  "dashboard.uploading_title": "Vos mesures sont envoyées au tableau de bord CodeCarbon.",
  "dashboard.how_measured": "Comment est-ce mesuré ?",

  // --- « Comment est-ce mesuré ? » -----------------------------------------
  "details.intro":
    "CodeCarbon estime la puissance de votre processeur (CPU), de votre carte graphique (GPU) " +
    "et de votre mémoire (RAM), multiplie l'énergie consommée par l'intensité carbone du réseau " +
    "électrique de votre pays, et en déduit les émissions en équivalent CO₂.",
  "details.cpu": "CPU : ",
  "details.gpu": "GPU : ",
  "details.ram": "RAM : ",
  "details.electricity": "Électricité : ",
  "details.threads_one": "{count} cœur logique",
  "details.threads_other": "{count} cœurs logiques",
  "details.unknown_model": "modèle inconnu",
  "details.unknown": "inconnu",
  "details.unknown_location": "lieu inconnu",
  "details.generic_tdp":
    "Votre processeur ne figure pas dans la base de CodeCarbon : un TDP générique est utilisé.",
  "details.ram_value": "{value} Go — {method}",
  "details.ram_estimate": "estimation",
  "details.no_hardware": "Lancez une mesure pour voir votre matériel.",
  "details.notes": "Remarques de CodeCarbon :",

  // --- Méthode de mesure du CPU (ligne d'état) -------------------------------
  "method.measured": "mesuré ({tool})",
  "method.load": "estimé d'après la charge",
  "method.load_tdp": "estimé d'après la charge ({tdp})",
  "method.constant": "estimation constante",
  "method.constant_tdp": "estimation constante ({tdp})",
  "method.unknown": "inconnu",
  "method.tdp": "TDP {value}",
  "status.cpu": "CPU : {method}",
  "status.gpu": "GPU : {method}",

  // --- Durées ----------------------------------------------------------------
  "duration.seconds": "{value} s",
  "duration.minutes": "{value} min",
  "duration.hours": "{value} h",
  "duration.hours_minutes": "{h} h {m} min",

  // --- Équivalents -----------------------------------------------------------
  "eq.car_km": "≈ {value} km en voiture",
  "eq.car_m": "≈ {value} m en voiture",
  "eq.phone_one": "≈ {value} charge de smartphone",
  "eq.phone_other": "≈ {value} charges de smartphone",
  "eq.phone_percent": "≈ {value} % d'une charge de smartphone",
  "eq.led_tv": "≈ {time} de télévision LED",
  "eq.streaming": "≈ {time} de streaming vidéo",
  "eq.hours_one": "{value} heure",
  "eq.hours_other": "{value} heures",
  "eq.minutes": "{value} min",
  "eq.seconds": "{value} s",

  // --- Historique ------------------------------------------------------------
  "history.empty": "Vos sessions apparaîtront ici dès que vous aurez arrêté une mesure.",
  "history.csv_off":
    "L'historique est désactivé dans les réglages : les nouvelles sessions ne sont pas enregistrées.",
  "history.today": "Aujourd'hui",
  "history.this_week": "Cette semaine",
  "history.all_time": "Depuis le début",
  "history.sessions_one": "{count} session",
  "history.sessions_other": "{count} sessions",
  "history.no_session": "aucune session",
  "history.tile_title": "{label} : {sessions}",
  "history.last_30_days": "30 derniers jours",
  "history.chart_label": "CO₂e par jour sur les 30 derniers jours",
  "history.chart_max": "max {value} / jour",
  "history.chart_empty": "Aucune session ces 30 derniers jours.",
  "history.day_today": "{day} (aujourd'hui)",
  "history.day_title": "{day} : {co2} CO₂e",
  "history.everyday": "En termes concrets",
  "history.everyday_scope": "depuis le début",
  "history.orders_of_magnitude": "Ordres de grandeur, d'après des facteurs moyens.",
  "history.sessions": "Sessions",
  "history.show_more": "Afficher plus",
  "history.skipped_one": "{count} ligne illisible",
  "history.skipped_other": "{count} lignes illisibles",
  "history.session": "session",
  "history.session_title": "{project} · moyenne {power}",

  // --- Réglages --------------------------------------------------------------
  "settings.measurement": "Mesure",
  "settings.startup": "Démarrage",
  "settings.language": "Langue",
  "settings.language_system": "Langue du système",
  "settings.interval": "Intervalle de mesure (secondes)",
  "settings.country": "Pays",
  "settings.country_placeholder": "Détecter via l'adresse IP",
  "settings.country_help":
    "Code ISO à 3 lettres (FRA, USA, DEU…). Laissez vide pour le détecter d'après votre adresse IP.",
  "settings.region": "Région (facultatif)",
  "settings.region_placeholder": "p. ex. california",
  "settings.region_help": "État ou province, pour les États-Unis et le Canada.",
  "settings.save_csv": "Conserver l'historique",
  "settings.save_csv_help": "Ajouter chaque session à emissions.csv",
  "settings.start_on_launch": "Mesurer dès le lancement",
  "settings.launch_at_login": "Lancer à l'ouverture de session",
  "settings.pue": "PUE (facultatif)",
  "settings.pue_help": "Efficacité énergétique du centre de données, pour les serveurs.",
  "settings.ci": "Intensité carbone imposée (g/kWh, facultatif)",
  "settings.ci_placeholder": "Celle de votre pays",
  "settings.api_endpoint": "Adresse de l'API",
  "settings.api_key": "Clé d'API",
  "settings.experiment_id": "Identifiant de l'expérience",
  "settings.manual_api":
    "Ou collez vous-même une clé d'API et un identifiant d'expérience issus du tableau de bord " +
    "(enregistrés avec le bouton « Enregistrer »).",
  "settings.save": "Enregistrer",
  "settings.saved": "Enregistré. Les changements de mesure s'appliqueront au prochain démarrage.",
  "settings.invalid": "Réglages non valides.",
  "settings.error.interval": "Entre 1 et 60 secondes.",
  "settings.error.country": "3 lettres (p. ex. FRA, USA, DEU), ou vide pour le détecter.",
  "settings.error.pue": "Entre 1,0 et 5,0, ou vide.",
  "settings.error.ci": "Entre 0 et 2000 g/kWh, ou vide.",
  "settings.error.https": "Doit commencer par https://",
  "settings.error.required": "Nécessaire pour l'envoi.",

  // --- Compte du tableau de bord CodeCarbon ----------------------------------
  "account.title": "Tableau de bord CodeCarbon",
  "account.pitch":
    "Envoyez vos sessions sur le tableau de bord gratuit de CodeCarbon pour les suivre " +
    "dans le temps et les partager.",
  "account.sign_in": "Se connecter ou créer un compte",
  "account.reopen": "Rouvrir la page de connexion",
  "account.finish_in_browser": "Terminez la connexion dans votre navigateur…",
  "account.checking": "Vérification de votre compte…",
  "account.engine_too_old":
    "La connexion au tableau de bord nécessite un moteur de mesure plus récent. " +
    "Vous pouvez toujours saisir une clé d'API dans les options avancées.",
  "account.signed_in_as": "Compte : {user}",
  "account.sign_out": "Se déconnecter",
  "account.organization": "Organisation",
  "account.project": "Projet",
  "account.experiment": "Expérience",
  "account.new_name.organization": "Nom de la nouvelle organisation",
  "account.new_name.project": "Nom du nouveau projet",
  "account.new_name.experiment": "Nom de la nouvelle expérience",
  "account.name_required.organization": "Veuillez saisir un nom pour la nouvelle organisation.",
  "account.name_required.project": "Veuillez saisir un nom pour le nouveau projet.",
  "account.name_required.experiment": "Veuillez saisir un nom pour la nouvelle expérience.",
  "account.create": "Créer",
  "account.refresh": "Actualiser",
  "account.saving": "Enregistrement…",
  "account.creating": "Création…",
  "account.loading": "Chargement…",
  "account.upload": "Envoyer mes sessions au tableau de bord",
  "account.uploads_to": "Envoi vers {target}",
  "account.uploads_off": "Envoi désactivé. Sélection : {target}",
  "account.choose_target": "Choisissez l'expérience qui recevra vos sessions.",
  "account.busy_hint":
    "Une mesure est en cours : les changements s'appliqueront à la prochaine mesure.",
  "account.create_new": "＋ Créer…",
  "account.unnamed": "(sans nom {id})",
  "account.no_organization": "Aucune organisation",
  "account.no_project": "Aucun projet",
  "account.no_experiment": "Aucune expérience",
  "account.choose_organization_first": "Choisissez d'abord une organisation",
  "account.choose_project_first": "Choisissez d'abord un projet",
  "account.choose_experiment": "Choisissez une expérience…",
  "account.your_account": "votre compte CodeCarbon",
  "account.experiment_id": "expérience {id}",
  "account.the_dashboard": "le tableau de bord CodeCarbon",

  // --- À propos --------------------------------------------------------------
  "about.tagline": "Mesurez la consommation électrique et les émissions de CO₂ de votre ordinateur.",
  "about.app": "Application",
  "about.agent": "Agent",
  "about.platform": "Plateforme",
  "about.link_desktop": "CodeCarbon Desktop sur GitHub",
  "about.link_library": "La bibliothèque CodeCarbon sur GitHub",
  "about.licence":
    "Distribué sous licence MIT. Les chiffres sont calculés par la bibliothèque CodeCarbon : " +
    "ce sont des estimations, pas des mesures de laboratoire.",
  "about.quit": "Quitter CodeCarbon",

  // --- Erreurs venant de Rust (src-tauri/src/error.rs) -------------------------
  "error.engine_not_ready": "Le moteur de mesure n'est pas encore prêt.",
  "error.engine_too_old":
    "Cette version du moteur de mesure ne permet pas de se connecter au tableau de bord.",
  "error.engine_unreachable": "Impossible de communiquer avec le moteur de mesure ({detail}).",
  "error.engine_restarting":
    "Le moteur de mesure s'est arrêté de façon inattendue et redémarre…",
  "error.engine_restarting_lost":
    "Le moteur de mesure s'est arrêté de façon inattendue et redémarre. " +
    "La mesure en cours a été perdue.",
  "error.engine_restarted_lost":
    "Le moteur de mesure a redémarré : la mesure en cours a été perdue.",
  "error.engine_failed": "Le moteur de mesure n'a pas pu démarrer ({detail}).",
  "error.measurement_failed": "Erreur de mesure : {detail}",
  "error.https_only": "Seuls les liens https:// peuvent être ouverts.",
  "error.open_link_failed": "Impossible d'ouvrir le lien ({detail}).",
  "error.browser_failed":
    "Impossible d'ouvrir votre navigateur ({detail}). Utilisez le lien ci-dessous.",
  "error.open_folder_failed": "Impossible d'ouvrir le dossier des données ({detail}).",
  "error.history_read_failed": "Impossible de lire l'historique ({detail}).",
  "error.settings_save_failed": "Impossible d'enregistrer les réglages ({detail}).",
  "error.launch_at_login_failed":
    "Réglages enregistrés, mais le lancement à l'ouverture de session n'a pas pu être modifié ({detail}).",
  "error.invalid_interval": "L'intervalle de mesure doit être compris entre 1 et 60 secondes.",
  "error.invalid_country": "Le code pays doit comporter 3 lettres (p. ex. FRA, USA, DEU).",
  "error.invalid_pue": "Le PUE doit être compris entre 1,0 et 5,0.",
  "error.invalid_carbon_intensity":
    "L'intensité carbone doit être comprise entre 0 et 2000 g/kWh.",
  "error.invalid_api_endpoint": "L'adresse de l'API doit commencer par https://",
  "error.api_credentials_missing":
    "L'envoi vers l'API nécessite une clé d'API et un identifiant d'expérience.",
  "error.name_required": "Veuillez saisir un nom.",
  "error.parent_required": "Choisissez d'abord où le créer.",
  "error.catalog_not_loaded": "La liste des expériences n'est pas encore chargée.",
  "error.experiment_gone": "Cette expérience n'est plus dans la liste. Actualisez-la.",
  "error.sign_in_failed": "La connexion a échoué.",
  "error.sign_in_rejected": "La connexion a échoué ({detail}).",
  "error.sign_in_again": "Veuillez vous reconnecter au tableau de bord.",
  "error.auth_unavailable": "La connexion n'est pas disponible avec ce moteur de mesure.",
  "error.dashboard_error": "Problème avec le tableau de bord CodeCarbon : {detail}",
  "error.insecure_login_url": "L'adresse de la page de connexion n'est pas sécurisée (https).",
  "error.empty_api_key": "Le tableau de bord a renvoyé une clé d'API vide.",
};
