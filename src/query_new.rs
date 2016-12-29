pub struct Query {
    before: Vec<char>,
    after: Vec<char>,
}

impl Query {
    pub fn new(query: Option<&str>) -> Self {
        Query {
            before: query.unwrap_or(&"").chars().collect(),
            after: Vec::new(),
        }
    }
    pub fn get_query(&self) -> String {
        self.before.iter().cloned().chain(self.after.iter().cloned().rev()).collect()
    }

    pub fn print_screen(&self) {
        // print the query to screen
        println!("Query = '{}'", self.get_query());
    }

//------------------------------------------------------------------------------
// Actions
//
    pub fn act_add_char(&mut self, ch: char) {
        self.before.push(ch);
    }


}

#[cfg(test)]
mod test {
    #[test]
    fn test_new_query() {
        let query1 = super::Query::new(None);
        assert_eq!(query1.get_query(), "");

        let query2 = super::Query::new(Some("abc"));
        assert_eq!(query2.get_query(), "abc");
    }
}
