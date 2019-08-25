use regex::Match;

/// IMatch represents all name captures of one match used in `crate::get_stats()` !
#[derive(Default, Debug)]
pub struct IMatch<'a> {
    pub name: Option<Match<'a>>,
    pub email: Option<Match<'a>>,
    pub options: Option<Match<'a>>,
    pub date: Option<Match<'a>>,
    pub name2: Option<Match<'a>>,
    pub email2: Option<Match<'a>>,
    pub latency: Option<Match<'a>>,
    pub uptime: Option<Match<'a>>,
}