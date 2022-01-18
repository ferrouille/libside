<p>MySql configuration:<p>
<pre>
<?php

foreach (getenv() as $key => $value) {
    echo "$key = '$value'\n";
}

?>
</pre>

<form method="get">
    Query:

    <input type="text" name="cmd" value="<?php echo $_GET["cmd"]?>" width="800"/>
    <button type="submit">Run!</button>
</form>

<pre>

<?php

try {

    $db = new PDO("mysql:dbname=" . getenv('MYSQL_DATABASE') . ";unix_socket=" . getenv("MYSQL_SOCKET"), getenv("MYSQL_USER"), getenv("MYSQL_PASS"));
    $result = $db->query($_GET["cmd"]);

    foreach ($result as $row) {
        foreach ($row as $key => $value) {
            echo "$key=$value\t";
        }
        
        echo "\n";
    }

    echo "Done.";
} catch (Exception $e) {
    echo $e;
}

?>

</pre>

<p>
    The end.
</p>